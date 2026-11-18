//! Redis connection pool management.
//!
//! # Why the pool is built off the async runtime's critical path
//!
//! `Manager::new` parses the URL and establishes a connection, and it is a
//! *blocking* call. Building it inside a `LazyLock` means the first caller pays
//! that cost inline. When Redis accepts the TCP connection and then never
//! answers — a wedged server, a firewall that drops the reply, a stopped
//! container behind a proxy — the handshake never completes and the caller
//! blocks forever. Measured by pointing the service at a socket that accepts and
//! stays silent: startup never reached the point of binding a listener, so the
//! deploy's health check failed and the service had to be rolled back.
//!
//! So the pool is built on a blocking thread with a timeout. A pool that cannot
//! be built is not fatal: the caller gets `Err` and the affected feature degrades
//! (the status store logs once and keeps counters in process), which is always
//! better than a process that will not start.

use std::sync::LazyLock;
use std::time::Duration;

use deadpool_redis::{Manager, Pool};
use tracing::{debug, error, info};

use crate::config::CONFIG;

/// How long building the pool may take before it is treated as unavailable.
///
/// Redis is either reachable in milliseconds or it is not; a slow answer means
/// something is wrong upstream and waiting longer only delays the listener.
const POOL_BUILD_TIMEOUT: Duration = Duration::from_secs(3);

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

    // `Manager::new` performs the handshake, so it runs on a dedicated blocking
    // thread rather than inline: a wedged Redis would otherwise hold this
    // `LazyLock` open forever and the process would never reach its listener.
    let (tx, rx) = std::sync::mpsc::channel();
    let url_for_build = redis_url.clone();
    std::thread::Builder::new()
        .name("redis-pool-init".to_string())
        .spawn(move || {
            let built = Manager::new(url_for_build)
                .map_err(|e| format!("Failed to create Redis manager: {e}"))
                .and_then(|manager| {
                    Pool::builder(manager)
                        .max_size(100)
                        .wait_timeout(Some(Duration::from_millis(200)))
                        .runtime(deadpool_redis::Runtime::Tokio1)
                        .build()
                        .map_err(|e| format!("Failed to create Redis connection pool: {e}"))
                });
            // The receiver is gone only if the timeout already fired.
            let _ = tx.send(built);
        })
        .map_err(|e| format!("Failed to spawn Redis pool initialiser: {e}"))?;

    match rx.recv_timeout(POOL_BUILD_TIMEOUT) {
        Ok(built) => built,
        Err(_) => {
            // The thread is left to finish or die on its own; it holds no lock and
            // touches no shared state, so leaking it on a wedged Redis is safe.
            error!(
                "Redis pool init exceeded {POOL_BUILD_TIMEOUT:?}; continuing without Redis.                  The dashboard will report in-process counters."
            );
            Err(format!(
                "Redis pool init timed out after {POOL_BUILD_TIMEOUT:?}"
            ))
        }
    }
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
