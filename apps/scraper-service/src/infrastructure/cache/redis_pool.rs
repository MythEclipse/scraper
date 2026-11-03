//! Redis connection pool management.

use std::sync::LazyLock;

use deadpool_redis::{Manager, Pool};
use tracing::{debug, error, info};

use crate::config::CONFIG;

static REDIS_POOL_INIT: LazyLock<Result<Pool, String>> = LazyLock::new(|| {
    let redis_url = if !CONFIG.redis_url.is_empty() {
        CONFIG.redis_url.clone()
    } else {
        let host = std::env::var("REDIS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
        let password = std::env::var("REDIS_PASSWORD").unwrap_or_default();
        if password.is_empty() {
            format!("redis://{}:{}", host, port)
        } else {
            format!("redis://:{}@{}:{}", password, host, port)
        }
    };

    info!("Initializing Redis connection pool for URL: {}", redis_url);

    Manager::new(redis_url)
        .map_err(|e| format!("Failed to create Redis manager: {}", e))
        .and_then(|manager| {
            Pool::builder(manager)
                .max_size(100)
                .wait_timeout(Some(std::time::Duration::from_millis(200)))
                .runtime(deadpool_redis::Runtime::Tokio1)
                .build()
                .map_err(|e| format!("Failed to create Redis connection pool: {}", e))
        })
});

pub fn get_redis_pool() -> Result<&'static Pool, String> {
    REDIS_POOL_INIT.as_ref().map_err(|e| e.clone())
}

pub fn redis_pool() -> Result<Pool, String> {
    get_redis_pool().cloned()
}

pub async fn get_redis_conn() -> Result<deadpool_redis::Connection, String> {
    let pool = get_redis_pool()?;
    let mut retries = 5;
    let mut wait = std::time::Duration::from_millis(100);

    loop {
        match pool.get().await {
            Ok(conn) => {
                debug!("Successfully retrieved Redis connection from pool.");
                return Ok(conn);
            }
            Err(e) => {
                if retries <= 0 {
                    error!("Failed to get Redis connection after retries: {:?}", e);
                    return Err(format!("Redis connection failed: {}", e));
                }
                debug!("Redis connection failed, retrying in {:?}: {:?}", wait, e);
                tokio::time::sleep(wait).await;
                wait = std::cmp::min(wait * 2, std::time::Duration::from_secs(5));
                retries -= 1;
            }
        }
    }
}
