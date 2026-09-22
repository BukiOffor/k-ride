use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use dotenvy::dotenv;
use std::time::Duration;
use crate::logs::ActivityLog;

use super::*;


pub async fn create_pool() -> crate::Pool {
    dotenv().ok();
    let db_url = std::env::var("DATABASE_URL").unwrap();
    // set up connection pool
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(db_url);
    bb8::Pool::builder()
        .max_size(10)
        .min_idle(Some(8))
        .connection_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(300))
        .build(config)
        .await
        .unwrap()
}

pub async fn start_log_workers(
    state: Arc<AppState>,
    receiver: async_channel::Receiver<ActivityLog>,
    log_workers: usize,
) {
    info!("Spawning {} log worker(s)", log_workers);
    for i in 1..=log_workers {
        let receiver = receiver.clone();
        let pool = state.pool.clone();
        tokio::spawn(async move {
            tracing::debug!("Log worker #{} started", i);
            while let Ok(first) = receiver.recv().await {
                let mut batch = vec![first];
                while batch.len() < 50 {
                    match receiver.try_recv() {
                        Ok(item) => batch.push(item),
                        Err(_) => break,
                    }
                }

                match pool.get().await {
                    Ok(mut conn) => {
                        if let Err(e) = crate::logs::services::emit_logs(&batch, &mut conn).await {
                            tracing::error!("Error emitting log batch: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Could not get connection from pool for log worker #{}: {}",
                            i,
                            e
                        );
                    }
                }
            }
            tracing::debug!("Log worker #{} stopped", i);
        });
    }
    info!("Spawned Log Workers Successfully");
}