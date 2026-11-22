//! OVRLEY's MCP server for AI agents.
//!
//! - [`protocol`] speaks MCP over standard input and output, in the `ovrley --mcp` process.
//! - [`link`] carries tool calls from that process to the running window, and runs them there on
//!   the UI thread.
//! - [`catalog`] defines what a tool is and lists them all. The tools live in the module of what
//!   they edit: documents, the overlay, the timeline, the render and Sync Doctor.
//!
//! There is no async runtime: std threads only.

pub mod catalog;
pub mod link;
pub mod protocol;

mod documents;
mod overlay;
mod render;
mod timeline;
mod video_sync;

#[cfg(test)]
mod testing {
    //! A window for the tests: a dispatcher over fake services, on this thread or on one of its own.

    use crate::catalog::{self, Answer, ToolError, ToolResult};
    use crate::link::{SocketLocation, UiJob, Window, WindowClosed};
    use ovrley_app::lib::template::ThemeColors;
    use ovrley_app::store::agent::{AgentReply, AgentResult};
    use ovrley_app::store::services::fakes::{fake_services, Fakes};
    use ovrley_app::store::{Dispatcher, Store};
    use serde_json::{json, Value};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    const PATIENCE: Duration = Duration::from_secs(5);

    /// A window on the test's thread.
    pub struct Editor {
        pub dispatcher: Dispatcher,
        pub fakes: Fakes,
    }

    /// A call that was started.
    pub struct Started {
        /// The answer, when the dispatch gave one.
        pub answer: Option<ToolResult>,
        pub reply: mpsc::Receiver<AgentResult>,
    }

    impl Editor {
        /// Agent edits on, and nothing open.
        pub fn new() -> Self {
            Self::with(Store::default(), json!({}))
        }

        pub fn with(mut store: Store, preferences: Value) -> Self {
            let (parts, fakes) = fake_services(preferences);
            store.agent_session.allow_agent_edits = true;
            store.theme = Some(ThemeColors {
                ice: "#86aaaa".into(),
                aqua: "#40e0d0".into(),
                teal: "#005b5b".into(),
                accent: "#ad5013".into(),
            });
            let dispatcher = Dispatcher::new(store, parts);
            dispatcher.dispatch(|_, _| {});
            Self { dispatcher, fakes }
        }

        pub fn start(&self, name: &str, arguments: Value) -> Started {
            let (sender, reply) = mpsc::channel();
            let name = name.to_string();
            let answer = self
                .dispatcher
                .dispatch_with_result(move |store, services| {
                    let reply = AgentReply::new(move |result| {
                        // A call answered in its dispatch may leave nobody to read this.
                        let _ = sender.send(result);
                    });
                    catalog::call(&name, arguments, store, services, reply)
                })
                .expect("no dispatch is running");
            let answer = match answer {
                Answer::Now(result) => Some(result),
                Answer::Later => None,
            };
            Started { answer, reply }
        }

        /// Runs worker results until `poll` gives a value.
        pub fn until<T>(&self, what: &str, mut poll: impl FnMut() -> Option<T>) -> T {
            let deadline = Instant::now() + PATIENCE;
            loop {
                self.fakes.ui_post.drain(&self.dispatcher);
                if let Some(value) = poll() {
                    return value;
                }
                assert!(Instant::now() < deadline, "timed out waiting for {what}");
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        pub fn finish(&self, started: Started) -> ToolResult {
            match started.answer {
                Some(result) => result,
                None => self
                    .until("the reply", || started.reply.try_recv().ok())
                    .map_err(ToolError::Failed),
            }
        }

        pub fn call(&self, name: &str, arguments: Value) -> ToolResult {
            self.finish(self.start(name, arguments))
        }

        pub fn ok(&self, name: &str, arguments: Value) -> Value {
            self.call(name, arguments)
                .unwrap_or_else(|error| panic!("{name} failed: {error}"))
        }

        pub fn failed(&self, name: &str, arguments: Value) -> String {
            match self.call(name, arguments) {
                Err(ToolError::Failed(message)) => message,
                other => panic!("{name} should have failed: {other:?}"),
            }
        }

        pub fn invalid(&self, name: &str, arguments: Value) -> String {
            match self.call(name, arguments) {
                Err(ToolError::InvalidParams(message)) => message,
                other => panic!("{name} should have refused its arguments: {other:?}"),
            }
        }

        pub fn undo_steps(&self) -> usize {
            self.dispatcher.with_store(|store| store.history.past.len())
        }
    }

    pub fn activity_fixture() -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ovrley_core/tests/fixtures/activity/test-ride.fit")
            .to_string_lossy()
            .into_owned()
    }

    pub fn with_activity(duration_seconds: f64) -> Store {
        use ovrley_app::store::slices::media::ActivitySummary;
        let mut store = Store::default();
        store.media.activity_summary = Some(ActivitySummary {
            duration_seconds,
            ..ActivitySummary::of(&ovrley_core::activity::schema::ParsedActivity::default())
        });
        store
    }

    pub fn video_metadata(path: &str, duration: f64) -> ovrley_core::media::SourceVideoMetadata {
        ovrley_core::media::SourceVideoMetadata {
            path: path.into(),
            duration: Some(duration),
            fps_num: Some(30),
            fps_den: Some(1),
            resolution: Some(ovrley_core::media::Resolution {
                width: 1920,
                height: 1080,
            }),
            has_audio: true,
            ..Default::default()
        }
    }

    /// A 600 s activity and clips given as id, offset and duration.
    pub fn with_videos(clips: &[(&str, f64, f64)]) -> Store {
        use ovrley_app::lib::video_program::create_imported_video;
        use ovrley_core::media::video_edits::VideoEdits;
        use std::rc::Rc;

        let mut store = with_activity(600.0);
        let (mut videos, mut edits) = (Vec::new(), Vec::new());
        for (order, &(id, offset_seconds, duration)) in (0..).zip(clips) {
            let metadata = video_metadata(&format!("/videos/{id}.mp4"), duration);
            videos.push(create_imported_video(&metadata, id.into(), order).expect("a valid video"));
            let clip_edits = VideoEdits {
                offset_seconds,
                ..VideoEdits::default()
            };
            edits.push((id.to_string(), clip_edits));
        }
        store.video_import.imported_videos = Rc::new(videos);
        store.video_import.video_edits = Rc::new(edits.into_iter().collect());
        store
    }

    /// A window whose UI thread is a thread of its own, as the socket reaches it.
    #[derive(Clone)]
    pub struct UiThread(mpsc::Sender<UiJob>);

    impl UiThread {
        pub fn start() -> Self {
            let (jobs, posted) = mpsc::channel::<UiJob>();
            std::thread::spawn(move || {
                let editor = Editor::new();
                loop {
                    match posted.recv_timeout(Duration::from_millis(2)) {
                        Ok(job) => job(&editor.dispatcher.handle()),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    editor.fakes.ui_post.drain(&editor.dispatcher);
                }
            });
            Self(jobs)
        }

        pub fn connections(&self) -> usize {
            let (sender, count) = mpsc::channel();
            let job: UiJob = Box::new(move |dispatch| {
                let connections = dispatch
                    .with_store(|store| store.agent_session.connections)
                    .expect("no dispatch is running");
                sender.send(connections).expect("the test waits");
            });
            self.post(job).expect("the UI thread runs");
            count.recv().expect("the UI thread answers")
        }
    }

    impl Window for UiThread {
        fn post(&self, job: UiJob) -> Result<(), WindowClosed> {
            self.0.send(job).map_err(|_closed| WindowClosed)
        }
    }

    pub fn socket(label: &str) -> SocketLocation {
        let id = std::process::id();
        if cfg!(windows) {
            return SocketLocation::Namespaced(format!("ovrley-mcp-test-{label}-{id}"));
        }
        // Short, because socket paths are limited to about 100 bytes.
        let directory = std::env::temp_dir().join(format!("om-{label}-{id}"));
        let _ = std::fs::remove_dir_all(&directory);
        SocketLocation::File(directory.join("s.sock"))
    }
}
