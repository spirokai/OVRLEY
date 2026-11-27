//! Playback through the composed window without a screen: the app's window, projections and timers
//! on Slint's testing backend with the system clock, over the futuristic HUD template and
//! `test-ride.fit`. The timeline scenario plays on the player's own timer; the video scenario plays
//! a 60 fps clip whose frames arrive as MDK's would, through the function the player calls. Reports
//! the update loop's dispatches, projections and their times, and the process's CPU time. The
//! testing backend draws nothing, so window rendering and texture uploads are not measured.

use ovrley_app::features::history::HistoryMode;
use ovrley_app::features::player::playback;
use ovrley_app::features::shell::{activate_parsed_activity, EditorBackgroundMode};
use ovrley_app::store::media::{ActivitySource, ImportedActivity};
use ovrley_app::store::services::fakes::{fake_services, Fakes};
use ovrley_app::store::{DispatchStats, Dispatcher, Store};
use ovrley_app::video::{create_imported_video, VideoEditsMap};
use ovrley_core::media::edits::VideoEdits;
use ovrley_core::media::probe::{Resolution, SourceVideoMetadata};
use ovrley_ui::AppWindow;
use serde_json::{json, Value};
use slint::platform::WindowEvent;
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const USAGE: &str = "Usage: benchmark_playback <root> timeline|video [seconds]";
const RIDE: &str = "crates/ovrley_core/tests/fixtures/activity/test-ride.fit";
const TEMPLATE: &str = "templates/futuristic-hud.json";
const START_SECOND: f64 = 60.0;
const WARMUP: Duration = Duration::from_secs(2);
const SETTLE_TIMEOUT: Duration = Duration::from_secs(60);
/// The window: 1600 × 1000 at a scale factor of 2, as on a Retina display.
const WINDOW: (f32, f32, f32) = (1600.0, 1000.0, 2.0);
const VIDEO_FPS: u32 = 60;
const VIDEO_SECONDS: f64 = 1200.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Timeline,
    Video,
}

/// The process's user and system CPU time, all threads.
fn cpu_time() -> Duration {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage fills the struct it is given.
    let usage = unsafe {
        assert_eq!(libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()), 0, "getrusage failed");
        usage.assume_init()
    };
    let time = |value: libc::timeval| Duration::from_secs(value.tv_sec as u64) + Duration::from_micros(value.tv_usec as u64);
    time(usage.ru_utime) + time(usage.ru_stime)
}

/// Where a measurement starts or ends.
#[derive(Clone, Copy)]
struct Mark {
    wall: Instant,
    cpu: Duration,
    second: f64,
}

impl Mark {
    fn now(dispatcher: &Dispatcher) -> Self {
        Self { wall: Instant::now(), cpu: cpu_time(), second: dispatcher.with_store(|store| store.editor.selected_second) }
    }
}

fn load(root: &Path, scenario: Scenario) -> Result<impl FnOnce(&mut Store, &mut ovrley_app::store::services::Services) + 'static, String> {
    let path = root.join(RIDE);
    let scratch = std::env::temp_dir().join(format!("ovrley-benchmark-playback-{}", std::process::id()));
    let paths = ovrley_core::paths::AppPaths::from_runtime_roots(scratch.clone(), scratch.clone(), scratch.join("runtime"));
    let activity = ovrley_core::import::load_activity(&paths, &path).map_err(|error| error.to_string())?;
    let imported = ImportedActivity::from_activity(activity)?;
    let template_path = root.join(TEMPLATE);
    let file: Value = serde_json::from_slice(&std::fs::read(&template_path).map_err(|error| format!("{}: {error}", template_path.display()))?)
        .map_err(|error| format!("{}: {error}", template_path.display()))?;
    let document = ovrley_core::template::load_template_document(&file).map_err(|error| error.to_string())?;
    let video = match scenario {
        Scenario::Timeline => None,
        Scenario::Video => {
            let metadata = SourceVideoMetadata {
                path: "/benchmark/ride.mp4".into(),
                duration: Some(VIDEO_SECONDS),
                fps_num: Some(VIDEO_FPS),
                fps_den: Some(1),
                resolution: Some(Resolution { width: 3840, height: 2160 }),
                has_audio: true,
                codec_name: Some("h264".into()),
                ..Default::default()
            };
            Some(create_imported_video(&metadata, "ride".into(), 0)?)
        }
    };
    Ok(move |store: &mut Store, _: &mut ovrley_app::store::services::Services| {
        store.bootstrap.set_startup_ready();
        store.history.set_mode(HistoryMode::Reset);
        store.template.global_defaults = Rc::new(document.global_defaults);
        store.editor.set_config(document.config);
        activate_parsed_activity(store, ActivitySource::File { path: path.display().to_string() }, imported);
        if let Some(video) = video {
            let edits: VideoEditsMap = [(video.id.clone(), VideoEdits::default())].into_iter().collect();
            ovrley_app::store::videos::commit_video_collection(store, vec![video], edits);
            // What the window's players report when they start on a hardware device.
            store.player.video_clock_available = true;
            store.editor_shell.set_editor_background_mode(EditorBackgroundMode::Video);
        }
        store.editor.set_selected_second(START_SECOND);
    })
}

/// Waits until the preview presents every widget it will, then plays: warm-up, then the measured
/// stretch, then the event loop quits.
struct Run {
    dispatcher: Dispatcher,
    fakes: Fakes,
    scenario: Scenario,
    measured: Duration,
    driver: slint::Timer,
    video_clock: slint::Timer,
    quiet_ticks: Cell<u32>,
    presented: Cell<usize>,
    settle_started: Instant,
    result: RefCell<Option<Result<Value, String>>>,
    marks: RefCell<Vec<Mark>>,
    stats: Cell<DispatchStats>,
}

impl Run {
    fn drive(self: &Rc<Self>) {
        let run = Rc::clone(self);
        self.driver.start(slint::TimerMode::Repeated, Duration::from_millis(10), move || run.settle_tick());
    }

    fn fail(&self, error: String) {
        *self.result.borrow_mut() = Some(Err(error));
        let _ = slint::quit_event_loop();
    }

    fn settle_tick(self: &Rc<Self>) {
        self.fakes.ui_post.drain(&self.dispatcher);
        let (presented, dirty, error) = self.dispatcher.with_store(|store| (store.preview.presented().len(), store.preview.is_dirty(), store.preview.error.clone()));
        if let Some(error) = error {
            return self.fail(error);
        }
        let quiet = presented > 0 && presented == self.presented.replace(presented) && !dirty && self.fakes.ui_post.posted.lock().unwrap().is_empty();
        self.quiet_ticks.set(if quiet { self.quiet_ticks.get() + 1 } else { 0 });
        if self.quiet_ticks.get() >= 30 {
            self.driver.stop();
            self.play();
        } else if self.settle_started.elapsed() > SETTLE_TIMEOUT {
            self.fail(format!("the preview did not settle; {presented} widgets presented"));
        }
    }

    fn play(self: &Rc<Self>) {
        let started = Instant::now();
        self.dispatcher.dispatch(move |store, _| playback::play(store, started));
        if self.scenario == Scenario::Video {
            let (piece, source_start) = self.dispatcher.with_store(|store| {
                let committed = &store.player.timeline.timeline().committed;
                let piece = &committed.program.pieces[0];
                (piece.id.clone(), piece.source_start_second + (store.editor.selected_second - piece.timeline_start_second))
            });
            let handle = self.dispatcher.handle();
            // MDK presents each frame of the clip; the player passes each on.
            self.video_clock.start(slint::TimerMode::Repeated, Duration::from_secs_f64(1.0 / f64::from(VIDEO_FPS)), move || {
                let second = source_start + started.elapsed().as_secs_f64();
                let frame = (second * f64::from(VIDEO_FPS)).floor() / f64::from(VIDEO_FPS);
                playback::video_frame_presented(&handle, piece.clone(), frame);
            });
        }
        let run = Rc::clone(self);
        slint::Timer::single_shot(WARMUP, move || {
            run.dispatcher.take_stats();
            run.marks.borrow_mut().push(Mark::now(&run.dispatcher));
            let run = Rc::clone(&run);
            slint::Timer::single_shot(run.measured, move || {
                run.stats.set(run.dispatcher.take_stats());
                run.marks.borrow_mut().push(Mark::now(&run.dispatcher));
                run.video_clock.stop();
                let summary = run.summary();
                *run.result.borrow_mut() = Some(summary);
                let _ = slint::quit_event_loop();
            });
        });
    }

    fn summary(&self) -> Result<Value, String> {
        let marks = self.marks.borrow();
        let [start, end] = marks.as_slice() else { return Err("the run has no start and end".into()) };
        let (playing, fps, widgets) = self.dispatcher.with_store(|store| {
            (
                store.editor.preview_playback_state == ovrley_app::store::editor::PreviewPlaybackState::Playing,
                playback::player_timing(store).effective_fps,
                store.editor.config.widget_ids().len(),
            )
        });
        if !playing {
            return Err("playback stopped before the measurement ended".into());
        }
        let stats = self.stats.get();
        let wall = (end.wall - start.wall).as_secs_f64();
        let frames = ((end.second - start.second) * fps).round();
        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
        let per_frame = |value: f64| value / frames.max(1.0);
        Ok(json!({
            "scenario": match self.scenario { Scenario::Timeline => "timeline", Scenario::Video => "video" },
            "widgets": widgets,
            "presented": self.presented.get(),
            "wall_seconds": wall,
            "preview_fps": fps,
            "frames": frames,
            "frames_per_second": frames / wall,
            "dispatches": stats.dispatches,
            "dispatches_per_frame": per_frame(stats.dispatches as f64),
            "projections": stats.projections,
            "update_ms_per_frame": per_frame(ms(stats.update)),
            "projection_ms_per_frame": per_frame(ms(stats.projection)),
            "update_loop_ms_per_second": (ms(stats.update) + ms(stats.projection)) / wall,
            "cpu_ms_per_second": ms(end.cpu - start.cpu) / wall,
            "cpu_ms_per_frame": per_frame(ms(end.cpu - start.cpu)),
        }))
    }
}

fn main() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (root, scenario, seconds) = match arguments.as_slice() {
        [root, scenario] => (root, scenario, "10"),
        [root, scenario, seconds] => (root, scenario, seconds.as_str()),
        _ => return Err(USAGE.into()),
    };
    let scenario = match scenario.as_str() {
        "timeline" => Scenario::Timeline,
        "video" => Scenario::Video,
        _ => return Err(USAGE.into()),
    };
    let measured = seconds.parse::<u64>().ok().filter(|seconds| *seconds > 0).map(Duration::from_secs).ok_or("seconds must be a positive integer")?;
    let root = PathBuf::from(root);

    // The window's thread runs at this class; without it macOS may run a command-line process's
    // main thread on the efficiency cores, and the timings vary by a quarter between runs.
    #[cfg(target_os = "macos")]
    // SAFETY: sets the calling thread's own scheduling class.
    unsafe {
        assert_eq!(libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0), 0, "the QoS class was refused");
    }
    i_slint_backend_testing::init_integration_test_with_system_time();
    let setup = load(&root, scenario)?;
    let window = AppWindow::new().map_err(|error| error.to_string())?;
    window.window().set_size(slint::LogicalSize::new(WINDOW.0, WINDOW.1));
    window.window().dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: WINDOW.2 });
    let (parts, fakes) = fake_services(json!({}));
    let store = Store::hydrate(&parts.preferences)?;
    let dispatcher = ovrley_ui::app::compose(&window, store, parts);
    dispatcher.dispatch(setup);
    window.show().map_err(|error| error.to_string())?;

    let run = Rc::new(Run {
        dispatcher,
        fakes,
        scenario,
        measured,
        driver: slint::Timer::default(),
        video_clock: slint::Timer::default(),
        quiet_ticks: Cell::new(0),
        presented: Cell::new(0),
        settle_started: Instant::now(),
        result: RefCell::new(None),
        marks: RefCell::new(Vec::new()),
        stats: Cell::default(),
    });
    run.drive();
    slint::run_event_loop().map_err(|error| error.to_string())?;
    let result = run.result.borrow_mut().take().ok_or("the event loop ended without a result")??;
    println!("{}", serde_json::to_string_pretty(&result).expect("the summary serializes"));
    Ok(())
}
