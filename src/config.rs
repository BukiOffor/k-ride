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
    info!("Spawning Log Workers: spawing {}", log_workers);
    (1..=log_workers).for_each(|i| {
        info!("Spawning Log Worker Number: {}", i);
        let receiver = receiver.clone();
        let pool = state.pool.clone();
        tokio::spawn(async move {
            let mut conn = pool.get()
                .await
                .expect("Could not get connection pool");
            while let Ok(payload) = receiver.recv().await {
                info!("got log = {:?}", payload);
                crate::logs::services::emit_log(payload, &mut conn)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Error emitting log: {}", e);
                    });
            }
        });
    });
    info!("Spawned Log Workers Successfully");
}