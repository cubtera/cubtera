//! In-memory registry of live log-broadcast channels, one per in-flight
//! [`cubtera_model::Run`] - the piece that turns `cubtera_app::ports::
//! LogSink`'s "forward each chunk somewhere" into "somewhere a client can
//! actually subscribe over HTTP" (P7: "live streaming instead of reading
//! a finished artifact after completion").
//!
//! Lifecycle: `routes::run::apply` calls [`LogHub::register`] right after
//! `RunUseCase::queue_apply`/`queue_apply_direct` mints the `Run` row,
//! passes the returned sink into the `tokio::spawn`ed
//! `RunUseCase::run_and_finish` call, and calls [`LogHub::unregister`]
//! once that finishes (success or error) - so the channel only exists
//! for exactly as long as the run is actually in flight.
//! `routes::run::log_stream` (`GET .../runs/{id}/log/stream`, SSE) checks
//! this hub first (live tail) and falls back to the finished run's stored
//! artifact (`Run::logs_ref` via `Store::get_artifact`) if the run has
//! already finished - see that handler for the fallback.
//!
//! Deliberately process-local, in-memory, not persisted: a server restart
//! losing an in-flight run's live subscribers (they still get the
//! finished artifact once it's done, or a "run not found" if the process
//! genuinely died mid-run) is an acceptable v3 P7 trade-off - `Run` rows
//! themselves are durable (`cubtera-store`), only the *live* tail isn't.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// Channel capacity for one run's live log broadcast - generous enough
/// that a slow SSE client falling behind a chatty `terraform apply` only
/// drops (`broadcast::error::RecvError::Lagged`, which
/// `routes::run::log_stream` treats as "skip ahead", not an error to the
/// HTTP client) rather than blocking the run itself; `run_and_finish`'s
/// sink call is a non-blocking `try_send`-shaped `broadcast::Sender::send`
/// (fails only if there are zero receivers, which is fine - nobody's
/// listening live).
const CHANNEL_CAPACITY: usize = 4096;

#[derive(Clone, Default)]
pub struct LogHub {
    channels: Arc<Mutex<HashMap<String, broadcast::Sender<Vec<u8>>>>>,
}

impl LogHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `run_id` as "in flight" and return a
    /// [`cubtera_app::ports::LogSink`]-shaped closure that forwards every
    /// chunk into this run's broadcast channel. Call once, right before
    /// spawning the background execution.
    pub fn register(&self, run_id: &str) -> Arc<cubtera_app::ports::LogSink> {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        self.channels
            .lock()
            .unwrap()
            .insert(run_id.to_string(), tx.clone());
        Arc::new(move |chunk: &[u8]| {
            // No receivers yet/anymore is not an error - it just means
            // nobody's tailing this run live right now.
            let _ = tx.send(chunk.to_vec());
        })
    }

    /// Subscribe to `run_id`'s live channel, if it's currently registered
    /// (i.e. the run is still in flight). `None` means either the run
    /// already finished or never existed - the caller falls back to the
    /// stored artifact either way.
    pub fn subscribe(&self, run_id: &str) -> Option<broadcast::Receiver<Vec<u8>>> {
        self.channels
            .lock()
            .unwrap()
            .get(run_id)
            .map(broadcast::Sender::subscribe)
    }

    /// Deregister `run_id` once its run has finished (successfully or
    /// not) - every existing subscriber still drains whatever's left in
    /// its own receiver; new subscribers from this point on fall back to
    /// the stored artifact.
    pub fn unregister(&self, run_id: &str) {
        self.channels.lock().unwrap().remove(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_then_subscribe_receives_forwarded_chunks() {
        let hub = LogHub::new();
        let sink = hub.register("run-1");

        let mut rx = hub.subscribe("run-1").expect("run-1 should be registered");
        sink(b"hello");
        sink(b"world");

        assert_eq!(rx.try_recv().unwrap(), b"hello");
        assert_eq!(rx.try_recv().unwrap(), b"world");
    }

    #[test]
    fn subscribe_returns_none_for_an_unregistered_or_finished_run() {
        let hub = LogHub::new();
        assert!(hub.subscribe("nope").is_none());

        let _sink = hub.register("run-1");
        hub.unregister("run-1");
        assert!(hub.subscribe("run-1").is_none());
    }

    #[test]
    fn sink_after_unregister_is_a_harmless_no_op() {
        let hub = LogHub::new();
        let sink = hub.register("run-1");
        hub.unregister("run-1");
        // No receivers, no channel entry anymore - must not panic.
        sink(b"too late");
    }
}
