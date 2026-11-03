//! Scheduled maintenance jobs — driven by `mytheclipse::cron`.
//!
//! The scraper runs a daily 02:00 UTC cache-cleanup job that enqueues
//! image-cache repair work onto the application job queue. The cron driver
//! comes from the mytheclipse core crate (`schedule` + `CronSchedule`); the
//! actual per-key repair work is queued through `mytheclipse-queue`.

use mytheclipse::cron::schedule;

/// Starts the daily maintenance jobs on a background task.
///
/// Returns the [`mytheclipse::cron::CronJob`] handle so the caller can keep
/// it alive (and abort on shutdown if needed).
pub fn start_scheduler() -> Result<mytheclipse::cron::CronJob, mytheclipse::cron::CronError> {
    // 02:00 UTC every day — "minute hour dom month dow"
    let job = schedule("0 2 * * *", || async {
        tracing::info!("[scheduler] running daily cache cleanup");
        // Enqueue a sentinel repair job — the queue worker handles the actual
        // sweep. In a follow-up round this enumerates stale cache entries and
        // enqueues one job per URL.
        let _ =
            crate::infrastructure::queue::enqueue_cache_repair("__daily_sweep__".to_string()).await;
        tracing::info!("[scheduler] daily cache cleanup enqueued");
    })?;
    tracing::info!("[scheduler] daily cache cleanup scheduled at 02:00 UTC");
    Ok(job)
}
