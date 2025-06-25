use std::{
    borrow::Borrow,
    collections::HashMap,
    hash::Hash,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use tokio::sync::{
    RwLock,
    mpsc::{Sender, channel},
};

use crate::{Context, plugin::StringLike, sqlite::SqliteContext};

pub struct Cache<K: Hash + Eq, V, E, F: FnMut(K) -> Result<(K, V), E>> {
    inner: HashMap<K, (V, Instant)>,
    expires_after: Duration,
    fetcher: F,
}

impl<K: Hash + Eq, V, E, F: FnMut(K) -> Result<(K, V), E>> Cache<K, V, E, F> {
    pub fn new(fetcher: F, expires_after: Duration) -> Self {
        Self {
            fetcher,
            expires_after,
            inner: HashMap::new(),
        }
    }

    pub fn get<Q>(&mut self, key: &Q, make_key: impl FnOnce(&Q) -> K) -> Result<Option<&V>, E>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if let Some((_, cached_at)) = self.inner.get(key) {
            let now = Instant::now();
            if now <= *cached_at || now.duration_since(*cached_at) < self.expires_after {
                return Ok(Some(&self.inner.get(key).unwrap().0));
            }
            self.inner.remove(key);
        }
        let key = make_key(key);
        let (k, v) = (self.fetcher)(key)?;
        Ok(Some(&self.inner.entry(k).or_insert((v, Instant::now())).0))
    }

    pub fn get_owned(&mut self, key: K) -> Result<Option<&V>, E> {
        if let Some((_, cached_at)) = self.inner.get(&key) {
            let now = Instant::now();
            if now <= *cached_at || now.duration_since(*cached_at) < self.expires_after {
                return Ok(Some(&self.inner.get(&key).unwrap().0));
            }
            self.inner.remove(&key);
        }
        let (k, v) = (self.fetcher)(key)?;
        Ok(Some(&self.inner.entry(k).or_insert((v, Instant::now())).0))
    }

    pub fn clean(&mut self) {
        self.inner
            .retain(|_, v| Instant::now().duration_since(v.1) < self.expires_after);
    }
}

pub struct HTTPResponse {
    pub result_code: u16,
    pub body: Vec<u8>,
    pub err: String,
    pub ttl: SystemTime,
}

pub struct HTTPCache {
    default_ttl: Duration,
    in_memory_cache_ttl: Duration,
    in_memory_cache: RwLock<HashMap<String, Arc<HTTPResponse>>>,
    waiting: RwLock<HashMap<String, Vec<Sender<Arc<HTTPResponse>>>>>,
    client: reqwest::Client,
}

impl HTTPCache {
    pub async fn init(&self, context: SqliteContext) -> rusqlite::Result<()> {
        crate::sqlite::await_execute(&context, "CREATE TABLE get_request_cache(url TEXT, ttl INTEGER, body BLOB, err TEXT, result_code INTEGER)", [].into()).await?;
        Ok(())
    }
    pub async fn get(
        me: Arc<RwLock<HTTPCache>>,
        context: &SqliteContext,
        url: impl Into<StringLike>,
        timeout: Option<Duration>,
        ttl: Option<Duration>,
    ) -> Arc<HTTPResponse> {
        let url = url.into();
        let reader = me.read().await;
        if let Some(v) = reader.waiting.write().await.get_mut(url.to_str()) {
            let (sender, mut receiver) = channel(1);
            v.push(sender);
            return receiver
                .recv()
                .await
                .expect("failed to receive...... this is bad");
        }
        let mut in_memory_cache = reader.in_memory_cache.write().await;
        if let Some(v) = in_memory_cache.get(url.to_str()) {
            if v.ttl >= SystemTime::now() {
                log::debug!("returning {url} from local cache");
                return v.clone();
            }
            in_memory_cache.remove(url.to_str());
        }
        drop(in_memory_cache);
        let params1 = Box::new([Box::new(url.clone()) as Box<_>]);
        let params = Box::new([Box::new(url.clone()) as Box<_>]);
        let ctx = context.clone();
        if let Ok(v) = crate::sqlite::await_query(
            context,
            "SELECT * FROM get_request_cache WHERE url = ?1",
            params1,
            move |row| {
                let ttl = row.get("ttl")?;
                let ttl = SystemTime::UNIX_EPOCH + Duration::from_secs(ttl);
                if ttl < SystemTime::now() {
                    log::debug!("database entry is to old :<");
                    crate::sqlite::execute(
                        &ctx,
                        "DELETE FROM get_request_cache WHERE url = ?1",
                        params,
                    );
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(HTTPResponse {
                    result_code: row.get("result_code")?,
                    body: row.get("body")?,
                    err: row.get("err")?,
                    ttl,
                })
            },
        )
        .await
        {
            let arc = Arc::new(v);
            reader
                .in_memory_cache
                .write()
                .await
                .insert(url.to_string(), arc.clone());
            log::debug!("returning {url} from db cache");
            return arc;
        }
        let (sender, mut receiver) = channel(1);
        reader
            .waiting
            .write()
            .await
            .insert(url.to_string(), vec![sender]);
        drop(reader);
        let ctx = context.clone();
        tokio::spawn(async move {
            let reader = me.read().await;
            log::debug!("fetching {url}");
            let res = reader.run_request(&url, timeout, ttl).await;
            let res = Arc::new(res);
            reader
                .in_memory_cache
                .write()
                .await
                .insert(url.to_string(), res.clone());
            if let Some(v) = reader.waiting.write().await.remove(url.to_str()) {
                for v in &v {
                    _ = v.try_send(res.clone());
                }
            }
            crate::sqlite::execute(
                &ctx,
                "INSERT INTO get_request_cache (url, result_code, body, err, ttl) values (?1, ?2, ?3, ?4, ?5)",
                [
                    Box::new(url) as Box<_>,
                    Box::new(res.result_code) as Box<_>,
                    Box::new(res.body.clone()) as Box<_>,
                    Box::new(res.err.clone()) as Box<_>,
                    Box::new(
                        res.ttl
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .expect("time went backwards")
                            .as_secs(),
                    ) as Box<_>,
                ]
                .into(),
            );
        });
        receiver
            .recv()
            .await
            .expect("failed to receive...... this is bad")
    }
    async fn run_request(
        &self,
        url: &str,
        timeout: Option<Duration>,
        ttl: Option<Duration>,
    ) -> HTTPResponse {
        let res = match self
            .client
            .get(url)
            .timeout(timeout.unwrap_or(Duration::from_secs(30)))
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return HTTPResponse {
                    result_code: 0,
                    body: Vec::new(),
                    err: format!("{e}"),
                    ttl: SystemTime::now() + ttl.unwrap_or(self.default_ttl),
                };
            }
        };
        let result_code = res.status().as_u16();
        let body = match res.bytes().await {
            Ok(v) => v,
            Err(e) => {
                return HTTPResponse {
                    result_code: 0,
                    body: Vec::new(),
                    err: format!("{e}"),
                    ttl: SystemTime::now() + ttl.unwrap_or(self.default_ttl),
                };
            }
        };
        HTTPResponse {
            result_code,
            body: body.into(),
            err: String::new(),
            ttl: SystemTime::now() + ttl.unwrap_or(self.default_ttl),
        }
    }

    pub fn new() -> Self {
        HTTPCache {
            default_ttl: Duration::from_secs(60 * 10),
            in_memory_cache_ttl: Duration::from_secs(120),
            in_memory_cache: RwLock::default(),
            client: reqwest::Client::new(),
            waiting: <_>::default(),
        }
    }

    pub async fn clean(&self) {
        self.in_memory_cache
            .write()
            .await
            .retain(|_, v| v.ttl >= SystemTime::now());
    }
}

pub async fn clean_caches(ctx: &Context) {
    crate::utils::DESKTOP_FILE_INFO_CACHE
        .write()
        .expect("desktop file cache is poisoned :<")
        .clean();
    ctx.http_cache.read().await.clean().await;
}
