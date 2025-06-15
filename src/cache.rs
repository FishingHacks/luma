use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    hash::Hash,
    time::{Duration, Instant},
};

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

    pub fn get_cow<Q>(&mut self, key: Cow<'_, Q>) -> Result<Option<&V>, E>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        if let Some((_, cached_at)) = self.inner.get(&key) {
            let now = Instant::now();
            if now <= *cached_at || now.duration_since(*cached_at) < self.expires_after {
                return Ok(Some(&self.inner.get(&key).unwrap().0));
            }
            self.inner.remove(&key);
        }
        let key = key.into_owned();
        let (k, v) = (self.fetcher)(key)?;
        Ok(Some(&self.inner.entry(k).or_insert((v, Instant::now())).0))
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
}
