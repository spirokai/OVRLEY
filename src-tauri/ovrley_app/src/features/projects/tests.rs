//! The project lifecycle against real `.oly` files in a temporary folder, with fake dialogs and
//! video tools (`useProjectLifecycle.test.jsx`, `projectCommands.test.jsx`).

use super::*;
use crate::lib::video_program::fixtures::metadata;
use crate::store::services::fakes::{fake_services, Fakes, WindowCall};
use crate::store::Dispatcher;
use std::path::PathBuf;
use std::sync::Mutex;

/// A temporary folder removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ovrley-projects-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    /// An empty file the fake video tools probe as a 60 s video.
    fn video(&self, name: &str, fakes: &Fakes) -> String {
        let path = self.file(name);
        std::fs::write(&path, b"").unwrap();
        fakes
            .video_tools
            .add_video(&path, metadata(&path, 60.0, (30, 1), 1920, 1080));
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup(dir: &TempDir) -> (Dispatcher, Fakes) {
    let (parts, fakes) =
        fake_services(json!({ LAST_PROJECT_DIRECTORY_KEY: dir.0.to_string_lossy() }));
    (Dispatcher::new(Store::default(), parts), fakes)
}

fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
    for _ in 0..600 {
        fakes.ui_post.drain(dispatcher);
        if dispatcher.with_store(&done) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out");
}

fn idle(store: &Store) -> bool {
    !store.projects.running && !store.projects.loading && store.telemetry_queue.running.is_none()
}

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ovrley_core/tests/fixtures/activity")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

type Replies = Arc<Mutex<Vec<AgentResult>>>;

fn reply(replies: &Replies) -> AgentReply {
    let log = Arc::clone(replies);
    AgentReply::new(move |result| log.lock().unwrap().push(result))
}

/// Imports videos quietly (one at a time) and waits for each.
fn import_videos(dispatcher: &Dispatcher, fakes: &Fakes, paths: &[String]) {
    for path in paths {
        let path = path.clone();
        dispatcher.dispatch(move |store, services| {
            crate::features::video_preview::import::start_import(
                store,
                services,
                vec![path.clone()],
                crate::features::video_preview::import::ImportRequest {
                    include: Some(vec![path]),
                    ..Default::default()
                },
            )
            .ok();
        });
        settle(dispatcher, fakes, |store| {
            store.video_import_review.review.is_none() && idle(store)
        });
    }
}

/// Saves to `path` as an agent would and waits for the write.
fn save_to(dispatcher: &Dispatcher, fakes: &Fakes, path: &str) -> Result<bool, String> {
    let result = Rc::new(std::cell::RefCell::new(None));
    let slot = Rc::clone(&result);
    let path = path.to_string();
    dispatcher.dispatch(move |store, services| {
        save_project(
            store,
            services,
            true,
            Some(path),
            Box::new(move |_, _, saved| *slot.borrow_mut() = Some(saved)),
        )
    });
    settle(dispatcher, fakes, |_| result.borrow().is_some());
    let saved = result.borrow_mut().take().unwrap();
    saved
}

fn open(
    dispatcher: &Dispatcher,
    fakes: &Fakes,
    path: &str,
    answer: Option<UnsavedAnswer>,
    skip: bool,
) -> Replies {
    let replies: Replies = Arc::default();
    let request = ProjectOpenRequest {
        skip_missing_sources: skip,
        reply: Some(reply(&replies)),
        from_startup: false,
    };
    let path = path.to_string();
    dispatcher
        .dispatch(move |store, services| open_project(store, services, path, answer, request));
    settle(dispatcher, fakes, |store| {
        store.projects.missing_source.is_some() || store.unsaved_confirm.is_open() || (idle(store) && !replies.lock().unwrap().is_empty())
    });
    replies
}

fn status(dispatcher: &Dispatcher) -> ProjectStatus {
    dispatcher.with_store(project_status)
}

#[test]
fn a_saved_project_opens_with_its_videos_activity_and_status() {
    let dir = TempDir::new("round-trip");
    let (dispatcher, fakes) = setup(&dir);
    let videos = [dir.video("a.mp4", &fakes), dir.video("b.mp4", &fakes)];
    import_videos(&dispatcher, &fakes, &videos);
    let activity = fixture("test-ride.fit");
    dispatcher.dispatch(move |store, services| {
        crate::features::app_shell::import_activity(store, services, activity, None)
    });
    settle(&dispatcher, &fakes, |store| !store.media.is_processing);
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(5.0));
    let saved_offsets = dispatcher.with_store(|store| {
        store
            .video_import
            .video_edits
            .values()
            .map(|edits| edits.offset_seconds)
            .collect::<Vec<_>>()
    });

    let project = dir.file("race.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    assert_eq!(status(&dispatcher), ProjectStatus::Saved);

    // A new project, then the saved one again.
    dispatcher.dispatch(|store, services| new_project(store, services, None, None));
    assert_eq!(status(&dispatcher), ProjectStatus::New);
    assert!(!dispatcher.with_store(|store| store.video_import.has_videos()));

    let replies = open(&dispatcher, &fakes, &project, None, false);
    let answer = replies.lock().unwrap()[0].clone().unwrap();
    assert_eq!(answer["project"]["status"], "Saved");
    assert_eq!(answer["project"]["name"], "race.oly");
    dispatcher.with_store(|store| {
        assert_eq!(store.video_import.imported_videos.len(), 2);
        let offsets: Vec<f64> = store
            .video_import
            .video_edits
            .values()
            .map(|edits| edits.offset_seconds)
            .collect();
        assert_eq!(
            offsets, saved_offsets,
            "activation does not move the saved placements"
        );
        assert!(store.media.parsed_activity.is_some());
        assert_eq!(store.editor.selected_second, 5.0);
        assert!(store.history.past.is_empty(), "the open resets the history");
        assert_eq!(
            store.editor_shell.editor_background_mode,
            crate::features::app_shell::EditorBackgroundMode::Video
        );
    });
    assert_eq!(status(&dispatcher), ProjectStatus::Saved);

    // A timeline move and an edit each modify it.
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(6.0));
    assert_eq!(status(&dispatcher), ProjectStatus::Modified);
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(5.0));
    assert_eq!(status(&dispatcher), ProjectStatus::Saved);
    dispatcher.dispatch(|store, _| store.media.set_lap_enabled(0, false));
    assert_eq!(status(&dispatcher), ProjectStatus::Modified);
}

#[test]
fn no_baseline_reads_unsaved_until_the_startup_dialog_is_dismissed() {
    let dir = TempDir::new("startup");
    let (dispatcher, fakes) = setup(&dir);
    assert_eq!(status(&dispatcher), ProjectStatus::Unsaved);
    dispatcher.dispatch(load_startup_projects);
    settle(&dispatcher, &fakes, |store| store.projects.startup.open);
    dispatcher.dispatch(|store, _| dismiss_startup_dialog(store));
    assert_eq!(status(&dispatcher), ProjectStatus::New);
    assert!(!dispatcher.with_store(|store| store.projects.startup.open));
}

#[test]
fn the_startup_dialog_lists_the_remembered_folder_and_opens_without_asking() {
    let dir = TempDir::new("startup-open");
    let (dispatcher, fakes) = setup(&dir);
    let project = dir.file("first.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    dispatcher.dispatch(|store, _| store.media.set_lap_enabled(3, false));
    dispatcher.dispatch(load_startup_projects);
    settle(&dispatcher, &fakes, |store| store.projects.startup.open);
    assert_eq!(
        dispatcher.with_store(|store| store
            .projects
            .startup
            .projects
            .iter()
            .map(|project| project.name.clone())
            .collect::<Vec<_>>()),
        ["first"]
    );
    let path = project.clone();
    dispatcher.dispatch(move |store, services| startup_open_project(store, services, path));
    settle(&dispatcher, &fakes, idle);
    dispatcher.with_store(|store| {
        assert!(
            !store.unsaved_confirm.is_open(),
            "the startup dialog's open does not ask"
        );
        assert!(!store.projects.startup.open);
        assert_eq!(
            store.projects.loaded_project_path.as_deref(),
            Some(project.as_str())
        );
    });
}

#[test]
fn opening_asks_about_unsaved_changes_and_cancel_keeps_the_session() {
    let dir = TempDir::new("ask-open");
    let (dispatcher, fakes) = setup(&dir);
    let project = dir.file("p.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(9.0));
    assert_eq!(status(&dispatcher), ProjectStatus::Modified);

    let replies = open(&dispatcher, &fakes, &project, None, false);
    assert!(dispatcher.with_store(|store| store.unsaved_confirm.is_open()));
    dispatcher
        .dispatch(|store, services| answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel));
    assert_eq!(replies.lock().unwrap()[0], Ok(json!({ "opened": false })));
    assert_eq!(
        dispatcher.with_store(|store| store.editor.selected_second),
        9.0
    );

    // Discard opens the saved state.
    let replies = open(
        &dispatcher,
        &fakes,
        &project,
        Some(UnsavedAnswer::Discard),
        false,
    );
    assert!(replies.lock().unwrap()[0].is_ok());
    assert_eq!(
        dispatcher.with_store(|store| store.editor.selected_second),
        0.0
    );
}

#[test]
fn new_project_save_answer_saves_first() {
    let dir = TempDir::new("ask-new");
    let (dispatcher, fakes) = setup(&dir);
    let project = dir.file("p.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(9.0));
    let replies: Replies = Arc::default();
    let agent = reply(&replies);
    dispatcher.dispatch(move |store, services| {
        new_project(store, services, Some(UnsavedAnswer::Save), Some(agent))
    });
    settle(&dispatcher, &fakes, |_| !replies.lock().unwrap().is_empty());
    assert_eq!(
        replies.lock().unwrap()[0].as_ref().unwrap()["created"],
        true
    );
    // The save wrote the moved playhead before the new project replaced it.
    let read = read_project(&Default::default(), &project).unwrap();
    assert_eq!(read.project.timeline.playhead_second, 9.0);
}

#[test]
fn an_agent_cannot_save_a_project_never_saved_through_the_question() {
    let dir = TempDir::new("agent-save");
    let (dispatcher, _) = setup(&dir);
    dispatcher.dispatch(|store, _| dismiss_startup_dialog(store));
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(4.0));
    let replies: Replies = Arc::default();
    let agent = reply(&replies);
    dispatcher.dispatch(move |store, services| {
        new_project(store, services, Some(UnsavedAnswer::Save), Some(agent))
    });
    assert_eq!(
        replies.lock().unwrap()[0],
        Err(AGENT_SAVE_NEEDS_PATH.into())
    );
}

#[test]
fn closing_asks_and_each_answer_does_what_it_says() {
    let dir = TempDir::new("close");
    let (dispatcher, fakes) = setup(&dir);
    dispatcher.dispatch(|store, _| dismiss_startup_dialog(store));
    dispatcher.dispatch(request_close);
    assert_eq!(
        *fakes.window.calls.borrow(),
        [WindowCall::Close],
        "no changes close at once"
    );

    let (dispatcher, fakes) = setup(&dir);
    dispatcher.dispatch(|store, _| dismiss_startup_dialog(store));
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(3.0));
    dispatcher.dispatch(request_close);
    dispatcher.dispatch(request_close);
    assert!(dispatcher.with_store(|store| store.unsaved_confirm.is_open()));
    dispatcher
        .dispatch(|store, services| answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel));
    assert!(fakes.window.calls.borrow().is_empty());
    assert!(
        !dispatcher.with_store(|store| store.close_guard.handling_close),
        "a later request asks again"
    );
    dispatcher.dispatch(request_close);
    dispatcher.dispatch(|store, services| {
        answer_unsaved_confirm(store, services, UnsavedAnswer::Discard)
    });
    assert_eq!(*fakes.window.calls.borrow(), [WindowCall::Close]);
}

#[test]
fn a_missing_single_video_asks_and_cancel_leaves_the_session() {
    let dir = TempDir::new("missing");
    let (dispatcher, fakes) = setup(&dir);
    let video = dir.video("only.mp4", &fakes);
    import_videos(&dispatcher, &fakes, std::slice::from_ref(&video));
    let project = dir.file("p.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    std::fs::remove_file(&video).unwrap();
    dispatcher.dispatch(|store, services| {
        new_project(store, services, Some(UnsavedAnswer::Discard), None)
    });

    let replies = open(&dispatcher, &fakes, &project, None, false);
    dispatcher.with_store(|store| {
        let missing = store
            .projects
            .missing_source
            .as_ref()
            .expect("the dialog asks");
        assert_eq!(
            (missing.role, missing.file_name.as_str()),
            (SourceRole::Video, "only.mp4")
        );
    });
    dispatcher.dispatch(|store, _| cancel_missing_source(store));
    assert_eq!(replies.lock().unwrap()[0], Ok(json!({ "opened": false })));
    assert_eq!(
        dispatcher.with_store(|store| store.projects.loaded_project_path.clone()),
        None,
        "the session stays new"
    );

    // Load Anyway loads it as missing, from its saved metadata.
    let replies = open(&dispatcher, &fakes, &project, None, false);
    dispatcher.dispatch(skip_missing_source);
    settle(&dispatcher, &fakes, |_| !replies.lock().unwrap().is_empty());
    dispatcher.with_store(|store| {
        assert_eq!(store.video_import.imported_videos.len(), 1);
        assert!(store.video_import.imported_videos[0].missing);
    });

    // An agent skips without asking.
    dispatcher.dispatch(|store, services| {
        new_project(store, services, Some(UnsavedAnswer::Discard), None)
    });
    let replies = open(&dispatcher, &fakes, &project, None, true);
    assert!(replies.lock().unwrap()[0].is_ok());
}

#[test]
fn locating_a_file_of_another_type_fails_the_load() {
    let dir = TempDir::new("locate");
    let (dispatcher, fakes) = setup(&dir);
    let video = dir.video("only.mp4", &fakes);
    import_videos(&dispatcher, &fakes, std::slice::from_ref(&video));
    let project = dir.file("p.oly");
    assert_eq!(save_to(&dispatcher, &fakes, &project), Ok(true));
    std::fs::remove_file(&video).unwrap();
    let replies = open(
        &dispatcher,
        &fakes,
        &project,
        Some(UnsavedAnswer::Discard),
        false,
    );
    dispatcher.dispatch(locate_missing_source);
    fakes.dialogs.answer(Some("/elsewhere/notes.txt"));
    assert_eq!(
        replies.lock().unwrap()[0],
        Err("Selected file is not a supported video source".into())
    );
    assert!(dispatcher.with_store(|store| {
        store.media.error_message.as_deref().is_some_and(|message| {
            message.ends_with("Selected file is not a supported video source")
        })
    }));

    // A moved file located by the user loads.
    let moved = dir.video("moved.mp4", &fakes);
    let replies = open(
        &dispatcher,
        &fakes,
        &project,
        Some(UnsavedAnswer::Discard),
        false,
    );
    dispatcher.dispatch(locate_missing_source);
    fakes.dialogs.answer(Some(&moved));
    settle(&dispatcher, &fakes, |_| !replies.lock().unwrap().is_empty());
    assert_eq!(
        dispatcher.with_store(|store| store.video_import.imported_videos[0].path.clone()),
        moved
    );
}

#[test]
fn a_failed_load_leaves_the_session_untouched() {
    let dir = TempDir::new("broken");
    let (dispatcher, fakes) = setup(&dir);
    dispatcher.dispatch(|store, _| dismiss_startup_dialog(store));
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(2.0));
    let broken = dir.file("broken.oly");
    std::fs::write(&broken, b"not a zip").unwrap();
    let replies = open(
        &dispatcher,
        &fakes,
        &broken,
        Some(UnsavedAnswer::Discard),
        false,
    );
    assert!(replies.lock().unwrap()[0].is_err());
    dispatcher.with_store(|store| {
        assert_eq!(store.editor.selected_second, 2.0);
        assert!(store
            .media
            .error_message
            .as_deref()
            .is_some_and(|message| message.starts_with("open project: ")));
    });
}

#[test]
fn busy_commands_are_refused() {
    let dir = TempDir::new("busy");
    let (dispatcher, _) = setup(&dir);
    let replies: Replies = Arc::default();
    let agent = reply(&replies);
    dispatcher.dispatch(move |store, services| {
        store.media.set_processing(true);
        new_project(store, services, None, Some(agent));
    });
    assert_eq!(replies.lock().unwrap()[0], Err(BUSY.into()));
}
