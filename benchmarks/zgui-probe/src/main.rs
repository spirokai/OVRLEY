//! GPU cost probe on zgui (a spike, not product code): what one presented frame of a maximized
//! window costs when the frame is driven by a signal, next to `ovrley_ui`'s `gpu_probe` example.
//!
//! `cargo run --manifest-path benchmarks/zgui-probe/Cargo.toml -- <mode> <fps>`
//! - `blank`: an empty window whose background colour a 30 fps signal flips between two
//!   indistinguishable shades, so every frame damages the whole window.
//! - `timeline`: an empty window with a primitive timeline along its bottom (track, three clips,
//!   a 2 px playhead) whose only change per frame is the playhead moving 7 px.
//! - `still`: the empty window, with the 30 fps signal written but read by nothing on screen.
//! - `video`: the video at `PROBE_VIDEO=<path>` playing (muted, from `PROBE_IDLE` on) into a
//!   `surface` element that fills the window but a 24 px margin, through `ovrley_playback` on the
//!   device opened the app's way (see `video.rs`). Frames come at the video's rate; `fps` is unused.
//! - `fps` 0 never writes the signal (zgui then draws no frame at all).
//!
//! The same environment as `gpu_probe`: `PROBE_SECONDS` (default 30), `PROBE_SIZE=<w>x<h>` (here in
//! logical pixels, zgui's unit) instead of maximizing, `PROBE_IDLE=<s>` before the frames start.
//! `ZGUI_PRESENTER=native` makes zgui present through its own swap chain with dirty rectangles
//! (DX12 `Present1`, Vulkan `VK_KHR_incremental_present`); unset, it copies its whole persistent
//! target into a wgpu swapchain image every frame (like `gpu_probe copy`).
//!
//! Every mode draws on the device the app opens (`video::open_device`), adopted by zgui.
//!
//! The probe prints frames presented per second and the CPU wall time zgui reports per frame, and
//! with `ZGUI_GPU_TIMING=1` zgui's own GPU time per pass, damaged pixels and draw calls. GPU time
//! as a whole is read from outside: `gpu-sample.mjs` (macOS, Windows).

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use zgui::prelude::*;
use zgui::render::FrameOutcome;
use zgui::runtime::{FrameProbe, Window};

mod video;

const PLAYHEAD_STEP: f32 = 7.0;
const PLAYHEAD_WIDTH: f32 = 2.0;

fn env_seconds(name: &str, default: u64) -> Duration {
    Duration::from_secs(
        std::env::var(name)
            .ok()
            .map_or(default, |value| value.parse().expect(name)),
    )
}

fn fixed_size() -> Option<(f32, f32)> {
    let value = std::env::var("PROBE_SIZE").ok()?;
    let (width, height) = value.split_once('x').expect("PROBE_SIZE=<w>x<h>");
    Some((
        width.parse().expect("width"),
        height.parse().expect("height"),
    ))
}

/// Counts presented frames and zgui's own per-frame wall times, printed once a second and, for the
/// whole run, when the application ends.
#[derive(Default)]
struct Counter {
    /// The whole run: presented frames, their wall time, their paint time, first and last.
    run: Cell<(u32, Duration, Duration)>,
    span: Cell<Option<(Instant, Instant)>>,
    presented: Cell<u32>,
    skipped: Cell<u32>,
    elapsed: Cell<Duration>,
    paint: Cell<Duration>,
    report_at: Cell<Option<Instant>>,
    /// Damaged device pixels and draw calls of the presented frames since the last report.
    stats: Cell<(u64, u64)>,
    /// zgui's own pass timing (`ZGUI_GPU_TIMING=1`) since the last report: results, then draws,
    /// groups, vectors and blit in ms; and the last frame read, as each result is seen twice.
    gpu: Cell<(u32, f64, f64, f64, f64)>,
    gpu_frame: Cell<Option<u64>>,
}

impl Counter {
    /// Starts the whole-run totals over, when the frames start.
    fn restart_run(&self) {
        self.run.set((0, Duration::ZERO, Duration::ZERO));
        self.span.set(None);
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        let (frames, elapsed, paint) = self.run.get();
        let Some((first, last)) = self.span.get() else {
            println!("probe: run 0 frames ({} skipped)", self.skipped.get());
            return;
        };
        let per = |total: Duration| total.as_secs_f64() * 1000.0 / f64::from(frames.max(1));
        println!(
            "probe: run {frames} frames over {:.1} s ({:.1} fps), cpu {:.2} ms/frame (paint+submit {:.2})",
            (last - first).as_secs_f64(),
            f64::from(frames.saturating_sub(1)) / (last - first).as_secs_f64().max(f64::EPSILON),
            per(elapsed),
            per(paint),
        );
    }
}

impl FrameProbe for Counter {
    fn frame_ended(&self, window: &Window) {
        if let Some(report) = window.last_frame_report() {
            match report.outcome {
                FrameOutcome::Presented(stats) => {
                    let (damage, draws) = self.stats.get();
                    self.stats.set((
                        damage + stats.damage_px,
                        draws + u64::from(stats.draw_calls),
                    ));
                    self.presented.set(self.presented.get() + 1);
                    self.elapsed.set(self.elapsed.get() + report.elapsed);
                    self.paint.set(self.paint.get() + report.paint_elapsed);
                    let (frames, elapsed, paint) = self.run.get();
                    self.run.set((
                        frames + 1,
                        elapsed + report.elapsed,
                        paint + report.paint_elapsed,
                    ));
                    let now = Instant::now();
                    let first = self.span.get().map_or(now, |(first, _)| first);
                    self.span.set(Some((first, now)));
                }
                _ => {
                    self.skipped.set(self.skipped.get() + 1)
                }
            }
        }
        let timing = window
            .renderer()
            .as_any()
            .and_then(|renderer| renderer.downcast_ref::<zgui_render_wgpu::WgpuRenderer>())
            .and_then(zgui_render_wgpu::WgpuRenderer::gpu_timing);
        if let Some(timing) = timing
            && self.gpu_frame.get() != Some(timing.frame)
        {
            self.gpu_frame.set(Some(timing.frame));
            let (n, draws, groups, vectors, blit) = self.gpu.get();
            self.gpu.set((
                n + 1,
                draws + timing.draws_ms,
                groups + timing.groups_ms,
                vectors + timing.vectors_ms,
                blit + timing.blit_ms,
            ));
        }
        let now = Instant::now();
        let report_at = self.report_at.get().unwrap_or(now + Duration::from_secs(1));
        self.report_at.set(Some(report_at));
        if now < report_at {
            return;
        }
        let frames = self.presented.get();
        if frames > 0 {
            let per = |total: Duration| total.as_secs_f64() * 1000.0 / f64::from(frames);
            println!(
                "probe: {frames} frames ({} skipped), cpu {:.2} ms/frame (paint+submit {:.2})",
                self.skipped.get(),
                per(self.elapsed.get()),
                per(self.paint.get()),
            );
            let (damage, draws) = self.stats.get();
            println!(
                "probe: damage {:.2} Mpx/frame, {:.1} draw calls/frame",
                damage as f64 / 1e6 / f64::from(frames),
                draws as f64 / f64::from(frames),
            );
        }
        let (n, draws, groups, vectors, blit) = self.gpu.get();
        if n > 0 {
            let per = |ms: f64| ms / f64::from(n);
            println!(
                "probe: zgui gpu {:.3} ms/frame: draws {:.3}, groups {:.3}, vectors {:.3}, blit {:.3} ({n} timed)",
                per(draws + groups + vectors + blit),
                per(draws),
                per(groups),
                per(vectors),
                per(blit),
            );
        }
        self.stats.set((0, 0));
        self.gpu.set((0, 0.0, 0.0, 0.0, 0.0));
        self.presented.set(0);
        self.skipped.set(0);
        self.elapsed.set(Duration::ZERO);
        self.paint.set(Duration::ZERO);
        self.report_at.set(Some(now + Duration::from_secs(1)));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Blank,
    Timeline,
    Still,
    Video,
}

#[component]
fn Probe(mode: Mode, fps: u32, counter: Rc<Counter>) -> impl IntoView {
    let tick = RwSignal::new(0_u64);
    let window = use_window();
    let windows = use_windows();
    let width = window.size();

    if mode == Mode::Video {
        let _start = RwSignal::new_local(set_timeout(env_seconds("PROBE_IDLE", 0), move || {
            println!("probe: frames start");
            counter.restart_run();
            video::play();
        }));
        let _drawn = RwSignal::new_local(set_interval(Duration::from_secs(1), || {
            println!("probe: video frames drawn {}", video::take_drawn());
        }));
    } else if fps > 0 {
        let interval = Duration::from_secs_f64(1.0 / f64::from(fps));
        let _start = RwSignal::new_local(set_timeout(env_seconds("PROBE_IDLE", 0), move || {
            println!("probe: frames start");
            counter.restart_run();
            let ticker = set_interval(interval, move || tick.update(|tick| *tick += 1));
            // Kept for the life of the window: dropping the handle cancels the interval.
            std::mem::forget(ticker);
        }));
    }
    let _quit = RwSignal::new_local(set_timeout(env_seconds("PROBE_SECONDS", 30), move || {
        if mode == Mode::Video {
            video::stop();
        }
        windows.quit()
    }));

    if mode == Mode::Timeline {
        let playhead = move || {
            let span = (width.get().width.0 - PLAYHEAD_WIDTH).max(PLAYHEAD_STEP);
            let x = (tick.get() as f32 * PLAYHEAD_STEP) % span;
            Some(format!("{x}px"))
        };
        AnyView::new(view! {
            column(class = "stage") {
                box(class = "fill") {}
                box(class = "timeline") {
                    box(class = "track") {}
                    box(class = "clip clip--a") {}
                    box(class = "clip clip--b") {}
                    box(class = "clip clip--c") {}
                    box(class = "playhead", style:left = playhead) {}
                }
            }
        })
    } else if mode == Mode::Video {
        let path = std::env::var("PROBE_VIDEO").expect("PROBE_VIDEO=<path>");
        AnyView::new(view! { column(class = "stage") { {video::element(&path)} } })
    } else if mode == Mode::Still {
        AnyView::new(view! { column(class = "stage") {} })
    } else {
        let background = move || {
            Some(
                if tick.get() % 2 == 0 {
                    "#1b1b1f"
                } else {
                    "#1b1b20"
                }
                .to_owned(),
            )
        };
        AnyView::new(view! {
            column(class = "stage", style:background-color = background) {}
        })
    }
}

const SHEET: &str = css!(
    ":root { display: flex; }
    .stage { flex: 1; background-color: #1b1b1f; }
    .fill { flex: 1; }
    .timeline { position: relative; height: 96px; background-color: #232329; }
    .track { position: absolute; left: 0; right: 0; top: 40px; height: 4px; background-color: #3a3a44; }
    .clip { position: absolute; top: 56px; height: 28px; border-radius: 4px; background-color: #355c9a; }
    .clip--a { left: 40px; width: 320px; }
    .clip--b { left: 420px; width: 180px; }
    .clip--c { left: 700px; width: 520px; }
    .video { flex: 1; margin: 24px; }
    .playhead { position: absolute; top: 0; bottom: 0; width: 2px; background-color: #e5484d; }"
);

fn main() -> Result<(), zgui::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map_or("blank", String::as_str).to_owned();
    let fps: u32 = args.get(1).map_or(30, |value| value.parse().expect("fps"));
    let probe_mode = match mode.as_str() {
        "blank" => Mode::Blank,
        "timeline" => Mode::Timeline,
        "still" => Mode::Still,
        "video" => Mode::Video,
        other => panic!("unknown mode {other}"),
    };
    println!(
        "probe: pid {} mode {mode} fps {fps} presenter {}",
        std::process::id(),
        std::env::var("ZGUI_PRESENTER").unwrap_or_else(|_| "wgpu".into())
    );
    let counter = Rc::new(Counter::default());
    let run_counter = counter.clone();
    let app = app()
        .with_title("gpu probe")
        // The app's chrome, which `gpu_probe`'s slint modes get from `select_backend`: the native
        // frame without its title bar on macOS, no decorations elsewhere.
        .with_decorations(if cfg!(target_os = "macos") {
            Decorations::NoTitleBar
        } else {
            Decorations::None
        })
        .with_stylesheet(SHEET)
        .with_probe(counter);
    // Every mode draws on the device the app opens (Metal, DX12 or Vulkan), as `gpu_probe`'s slint
    // modes do; zgui's own choice of backend may differ.
    let app = app.with_renderer(video::factory(video::open_device()));
    let app = match fixed_size() {
        Some((width, height)) => app.with_size(width, height),
        None => app.with_maximized(true),
    };
    app.run(move || view! { Probe(mode = probe_mode, fps = fps, counter = run_counter.clone()) })
}
