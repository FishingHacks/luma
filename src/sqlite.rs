use std::{
    any::Any,
    sync::{Arc, OnceLock},
};

use rusqlite::{Connection, Result, Row, ToSql, params_from_iter};
use tokio::sync::mpsc::{Sender, UnboundedSender, channel, unbounded_channel};

use crate::{plugin::StringLike, utils};

type ProcessFunc = dyn Send + FnOnce(&Row<'_>) -> Result<Box<dyn Any + Send>>;

type Params = Box<[Box<dyn ToSql + Send>]>;

enum SqliteRequest {
    Query {
        query: StringLike,
        params: Params,
        process: Box<ProcessFunc>,
        responder: Sender<Result<Box<dyn Any + Send>>>,
    },
    Execute {
        query: StringLike,
        params: Params,
        responder: Option<Sender<Result<usize>>>,
    },
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct SqliteContext(Arc<UnboundedSender<SqliteRequest>>);

pub struct SqliteDeinitializer(Arc<UnboundedSender<SqliteRequest>>);
impl Drop for SqliteDeinitializer {
    fn drop(&mut self) {
        log::debug!("requesting to close sqlite cache");
        _ = self.0.send(SqliteRequest::Shutdown);
    }
}

pub fn init() -> Result<(SqliteContext, SqliteDeinitializer)> {
    let connection = Connection::open(utils::DATA_DIR.join("cache.sqlite"))?;
    let (sender, mut receiver) = unbounded_channel();
    let sender = Arc::new(sender);
    std::thread::spawn(move || {
        log::debug!("initialized sqlite cache");
        loop {
            let Some(request) = receiver.blocking_recv() else {
                log::debug!("closing sqlite cache");
                return;
            };
            match request {
                SqliteRequest::Query {
                    query,
                    params,
                    process,
                    responder,
                } => {
                    let result =
                        connection.query_row(&query, params_from_iter(params.iter()), process);
                    // if the channel is closed, the recipient probably doesn't care
                    // anymore, which is why nothing goes wrong in that case, so nothing
                    // gets logged.
                    _ = responder.try_send(result);
                }
                SqliteRequest::Execute {
                    query,
                    params,
                    responder,
                } => {
                    let result = connection.execute(&query, params_from_iter(params.iter()));
                    if let Some(responder) = responder {
                        // if the channel is closed, the recipient probably doesn't care
                        // anymore, which is why nothing goes wrong in that case, so nothing
                        // gets logged.
                        _ = responder.try_send(result);
                    }
                }
                SqliteRequest::Shutdown => {
                    _ = connection.close();
                    return;
                }
            }
        }
    });
    Ok((SqliteContext(sender.clone()), SqliteDeinitializer(sender)))
}

pub fn execute(
    context: &SqliteContext,
    query: impl Into<StringLike>,
    params: Box<[Box<dyn ToSql + Send>]>,
) {
    context
        .0
        .send(SqliteRequest::Execute {
            query: query.into(),
            params,
            responder: None,
        })
        .expect("async-sqlite closed");
}

/// returns the number of rows changed
pub async fn await_execute(
    context: &SqliteContext,
    query: impl Into<StringLike>,
    params: Box<[Box<dyn ToSql + Send>]>,
) -> Result<usize> {
    // if async-sqlite was closed, the application is about to exit anyway.
    let (sender, mut receiver) = channel(1);
    context
        .0
        .send(SqliteRequest::Execute {
            query: query.into(),
            params,
            responder: Some(sender),
        })
        .expect("async-sqlite closed");
    receiver
        .recv()
        .await
        .unwrap_or(Err(rusqlite::Error::QueryReturnedNoRows))
}

pub async fn await_query<T: Send + 'static, F: Send + 'static + FnOnce(&Row<'_>) -> Result<T>>(
    context: &SqliteContext,
    query: impl Into<StringLike>,
    params: Box<[Box<dyn ToSql + Send>]>,
    f: F,
) -> Result<T> {
    // if async-sqlite was closed, the application is about to exit anyway.
    let (sender, mut receiver) = channel(1);
    context
        .0
        .send(SqliteRequest::Query {
            query: query.into(),
            params,
            process: Box::new(move |row| Ok(Box::new(f(row)?))),
            responder: sender,
        })
        .expect("async-sqlite closed");
    let v = receiver
        .recv()
        .await
        .unwrap_or(Err(rusqlite::Error::QueryReturnedNoRows))?;
    Ok(*v.downcast().expect("these types *should always* match"))
}
