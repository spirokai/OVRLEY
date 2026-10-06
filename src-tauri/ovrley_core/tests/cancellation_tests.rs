//! Render cancellation lifecycle tests.
//!
//! Exercises the native service boundary with controlled preparation and cleanup.

use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::encode::progress::RenderController;
use ovrley_core::error::CoreError;
use ovrley_core::render_jobs::execution::RenderExecutionService;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Double cancel is idempotent: no panic, no deadlock.
#[test]
fn double_cancel_is_idempotent() {
    let controller = RenderExecutionService::default();
    let session = controller.reserve().unwrap();
    session.begin_item(100, "test_double_cancel").unwrap();

    let _ = controller.cancel();
    let _ = controller.cancel();

    let progress = controller.progress();
    assert_eq!(progress.status, "cancelling");
    assert!(progress.busy);
    assert!(matches!(
        session.complete(Err(CoreError::Cancelled)),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(controller.progress().status, "cancelled");
}

/// Cancellation and panics retain ownership through native cleanup.
#[test]
fn cancellation_or_panic_during_preparation_retains_ownership_until_cleanup() {
    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<RenderProgress>>);
    impl ProgressSink for RecordingSink {
        fn emit_progress(&self, progress: &RenderProgress) {
            self.0.lock().unwrap().push(progress.clone());
        }
    }
    struct ControlledCleanup {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Drop for ControlledCleanup {
        fn drop(&mut self) {
            self.entered.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }

    for cancel in [true, false] {
        let pending = if cancel { "cancelling" } else { "preparing" };
        let terminal = if cancel { "cancelled" } else { "error" };
        let sink = Arc::new(RecordingSink::default());
        let controller = RenderController::with_sink(sink.clone());
        let service = RenderExecutionService::with_controller(controller.clone());
        let (prepared_tx, prepared_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (cleanup_tx, cleanup_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let encoder_started = Arc::new(AtomicBool::new(false));
        let encoder = encoder_started.clone();
        let session = service.reserve().unwrap();
        service
            .dispatch(session, move |session| {
                let _cleanup = ControlledCleanup {
                    entered: cleanup_tx,
                    release: release_rx,
                };
                prepared_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                if !cancel {
                    panic!("Preparation failed");
                }
                session.begin_item(30, "Starting prepared item")?;
                session.check_cancelled()?;
                encoder.store(true, Ordering::SeqCst);
                Ok("output.mov".into())
            })
            .unwrap();

        prepared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(service.progress().status, "preparing");
        assert!(service.reserve().is_err());
        if cancel {
            assert!(service.cancel());
        }
        assert_eq!(service.progress().status, pending);
        assert!(service.reserve().is_err());
        resume_tx.send(()).unwrap();
        cleanup_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!encoder_started.load(Ordering::SeqCst));
        assert!(service.progress().busy);
        assert_eq!(service.progress().status, pending);
        assert!(service.reserve().is_err());
        release_tx.send(()).unwrap();
        // The native service joins its worker; events do not own it.
        drop(service);
        let progress = controller.progress();
        assert_eq!(progress.status, terminal);
        if !cancel {
            assert!(progress.message.contains("worker panicked"));
        }
        assert!(!progress.busy);
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.status == terminal)
                .count(),
            1
        );
        assert!(!controller.cancel());

        let service = RenderExecutionService::with_controller(controller);
        let next = service.reserve().unwrap();
        next.begin_item(1, "Next operation").unwrap();
        next.complete(Ok("next.mov".into())).unwrap();
        assert_eq!(service.progress().status, "complete");
    }
}
