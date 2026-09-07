//! One FFmpeg invocation pairs post-filter showinfo ordinals/PTS with raw gray frames.
//! FFmpeg's normal input timestamp offset maps container start to media time zero;
//! trim preserves that timeline. No input seek or setpts resets window timestamps.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{AnalysisError, AnalysisResult, AnalysisSettings, Cancellation};
use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};

pub(crate) struct GrayFrame {
    pub seconds: f64,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

struct FrameInfo {
    ordinal: usize,
    seconds: f64,
    width: usize,
    height: usize,
}

pub(super) struct Decoder {
    child: Child,
    frames: Option<Receiver<AnalysisResult<GrayFrame>>>,
    reader: Option<JoinHandle<AnalysisResult<usize>>>,
    diagnostics: Option<JoinHandle<AnalysisResult<(usize, String)>>>,
    diagnostic_result: Option<(usize, String)>,
    finished: bool,
}

impl Decoder {
    pub fn start(root: &Path, path: &Path, settings: &AnalysisSettings) -> AnalysisResult<Self> {
        let binary = resolve_ffmpeg_binary(root)?;
        let mut command = Command::new(binary);
        configure_ffmpeg_command(&mut command);
        let edge = settings.long_edge_pixels;
        let end = settings
            .end_seconds
            .map(|end| format!(":end={end}"))
            .unwrap_or_default();
        // Autorotation occurs before this graph. select never synthesizes duplicates.
        // Decode/discard all preroll: slower than seeking, but retains original media time.
        let filter = format!(
            "trim=start={}{},select='isnan(prev_selected_t)+gte(t-prev_selected_t,{})',scale=w='if(gte(iw,ih),min(iw,{edge}),-1)':h='if(gte(iw,ih),-1,min(ih,{edge}))',format=gray,showinfo=checksum=0",
            settings.start_seconds, end, 1.0 / settings.frames_per_second,
        );
        command
            .args([
                "-hide_banner",
                "-nostdin",
                "-nostats",
                "-loglevel",
                "info",
                // Decode optimizations: use hardware acceleration when available,
                // multi-threaded decoding, and fast bilinear scaling. Motion
                // estimation does not need high-quality scaling.
                "-hwaccel",
                "auto",
                "-threads",
                "0",
                "-sws_flags",
                "fast_bilinear",
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                &filter,
                "-fps_mode",
                "passthrough",
                // Allow multi-threaded filtering/output; rawvideo encoding itself
                // does not benefit, but the filter graph (scale/showinfo) does.
                "-threads",
                "0",
                "-c:v",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (info_tx, info_rx) = mpsc::sync_channel(2);
        let (frame_tx, frames) = mpsc::sync_channel(2);
        let mut decoder = Self {
            child,
            frames: Some(frames),
            reader: None,
            diagnostics: None,
            diagnostic_result: None,
            finished: false,
        };
        decoder.diagnostics = Some(thread::Builder::new().name("motion-pts".into()).spawn(
            move || {
                let mut count = 0;
                let mut tail = String::new();
                for line in BufReader::new(stderr).lines() {
                    let line = line?;
                    if let Some(info) = parse_showinfo(&line)? {
                        if info.ordinal != count {
                            return Err(AnalysisError::Decode("showinfo ordinal mismatch".into()));
                        }
                        count += 1;
                        if info_tx.send(info).is_err() {
                            break;
                        }
                    } else {
                        // Keep diagnostics bounded even for a long corrupt input.
                        tail.push_str(&line);
                        tail.push('\n');
                        if tail.len() > 8192 {
                            let mut boundary = tail.len() - 8192;
                            while !tail.is_char_boundary(boundary) {
                                boundary += 1;
                            }
                            tail.drain(..boundary);
                        }
                    }
                }
                Ok((count, tail))
            },
        )?);
        decoder.reader = Some(thread::Builder::new().name("motion-frames".into()).spawn(
            move || {
                let mut stdout = BufReader::new(stdout);
                let mut count = 0;
                for info in info_rx {
                    let mut pixels = vec![0; info.width * info.height];
                    if let Err(error) = stdout.read_exact(&mut pixels) {
                        let _ = frame_tx.send(Err(AnalysisError::Decode(format!(
                            "short raw frame {count}: {error}"
                        ))));
                        return Ok(count);
                    }
                    let frame = GrayFrame {
                        seconds: info.seconds,
                        width: info.width,
                        height: info.height,
                        pixels,
                    };
                    if frame_tx.send(Ok(frame)).is_err() {
                        return Ok(count);
                    }
                    count += 1;
                }
                let mut extra = [0];
                if stdout.read(&mut extra)? != 0 {
                    return Err(AnalysisError::Decode(
                        "raw frame without timestamp metadata".into(),
                    ));
                }
                Ok(count)
            },
        )?);
        Ok(decoder)
    }

    pub fn next(&mut self, cancellation: &Cancellation) -> AnalysisResult<Option<GrayFrame>> {
        loop {
            if cancellation.is_cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            match self
                .frames
                .as_ref()
                .expect("active decoder")
                .recv_timeout(Duration::from_millis(25))
            {
                Ok(frame) => return frame.map(Some),
                Err(RecvTimeoutError::Timeout) => {
                    // A failed stderr parser can leave stdout blocked in FFmpeg; terminate
                    // before waiting for readers, rather than waiting for pipe EOF forever.
                    if self
                        .diagnostics
                        .as_ref()
                        .is_some_and(|worker| worker.is_finished())
                    {
                        self.diagnostic_result =
                            Some(join(self.diagnostics.take().expect("diagnostics worker"))?);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let count = join(self.reader.take().expect("frame reader"))?;
                    let (metadata_count, tail) = match self.diagnostic_result.take() {
                        Some(result) => result,
                        None => join(self.diagnostics.take().expect("diagnostics reader"))?,
                    };
                    let status = self.child.wait()?;
                    self.finished = true;
                    if !status.success() {
                        return Err(AnalysisError::Decode(format!("FFmpeg {status}: {tail}")));
                    }
                    if count != metadata_count {
                        return Err(AnalysisError::Decode(
                            "frame/timestamp count mismatch".into(),
                        ));
                    }
                    if count == 0 {
                        return Err(AnalysisError::Decode(
                            "no decoded frames in selected window".into(),
                        ));
                    }
                    return Ok(None);
                }
            }
        }
    }
}

impl Decoder {
    fn stop_and_join(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
        }
        // Unblock a full frame queue before joining. Killing FFmpeg unblocks pipe reads.
        self.frames.take();
        if let Some(worker) = self.reader.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.diagnostics.take() {
            let _ = worker.join();
        }
        let _ = self.child.wait();
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn join<T>(worker: JoinHandle<AnalysisResult<T>>) -> AnalysisResult<T> {
    worker
        .join()
        .map_err(|_| AnalysisError::Decode("decoder reader panicked".into()))?
}

fn parse_showinfo(line: &str) -> AnalysisResult<Option<FrameInfo>> {
    if !line.contains("Parsed_showinfo_") || !line.contains(" n:") {
        return Ok(None);
    }
    let field = |key: &str| -> AnalysisResult<&str> {
        line.split_once(key)
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .ok_or_else(|| AnalysisError::Decode(format!("missing showinfo {key}")))
    };
    let malformed = || AnalysisError::Decode("malformed showinfo frame metadata".into());
    let ordinal = field(" n:")?.parse().map_err(|_| malformed())?;
    let seconds: f64 = field(" pts_time:")?.parse().map_err(|_| malformed())?;
    let (width, height) = field(" s:")?.split_once('x').ok_or_else(malformed)?;
    let width: usize = width.parse().map_err(|_| malformed())?;
    let height: usize = height.parse().map_err(|_| malformed())?;
    if !seconds.is_finite()
        || width == 0
        || height == 0
        || width > 1280
        || height > 1280
        || field(" fmt:")? != "gray"
    {
        return Err(malformed());
    }
    Ok(Some(FrameInfo {
        ordinal,
        seconds,
        width,
        height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn create() -> Self {
            static SEQUENCE: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "ovrley-motion-{}-{}.mp4",
                std::process::id(),
                SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let mut command = Command::new(resolve_ffmpeg_binary(&root()).unwrap());
            configure_ffmpeg_command(&mut command);
            let output = command
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=96x64:rate=20:duration=2",
                    "-vf",
                    "select='not(eq(mod(n,3),1))',setpts=PTS+5/TB",
                    "-fps_mode",
                    "passthrough",
                    "-c:v",
                    "mpeg4",
                    "-bf",
                    "2",
                    "-video_track_timescale",
                    "1000",
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn settings() -> AnalysisSettings {
        AnalysisSettings {
            frames_per_second: 40.0,
            long_edge_pixels: 96,
            start_seconds: 0.0,
            end_seconds: None,
        }
    }

    #[test]
    fn vfr_nonzero_container_start_and_window_keep_original_media_time() {
        let fixture = Fixture::create();
        let ffprobe = resolve_ffmpeg_binary(&root())
            .unwrap()
            .with_file_name(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            });
        let mut command = Command::new(ffprobe);
        configure_ffmpeg_command(&mut command);
        let output = command
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_frames",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(&fixture.0)
            .output()
            .unwrap();
        assert!(output.status.success());
        let probe: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let origin: f64 = probe["format"]["start_time"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(origin >= 5.0);
        let expected: Vec<f64> = probe["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                frame["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    - origin
            })
            .collect();
        let cancellation = Cancellation::default();
        let mut decoder = Decoder::start(&root(), &fixture.0, &settings()).unwrap();
        let mut actual = Vec::new();
        while let Some(frame) = decoder.next(&cancellation).unwrap() {
            assert_eq!(
                (frame.width, frame.height, frame.pixels.len()),
                (96, 64, 96 * 64)
            );
            actual.push(frame.seconds);
        }
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert!((actual - expected).abs() < 1e-5);
        }
        assert!(actual
            .windows(3)
            .any(|p| ((p[2] - p[1]) - (p[1] - p[0])).abs() > 0.01));
        let mut window = settings();
        window.start_seconds = 0.7;
        window.end_seconds = Some(1.5);
        let mut decoder = Decoder::start(&root(), &fixture.0, &window).unwrap();
        let mut selected = Vec::new();
        while let Some(frame) = decoder.next(&cancellation).unwrap() {
            selected.push(frame.seconds);
        }
        assert_eq!(
            selected,
            actual
                .into_iter()
                .filter(|t| *t >= 0.7 && *t < 1.5)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cancellation_reaps_child_and_joins_backpressured_readers() {
        let fixture = Fixture::create();
        let mut decoder = Decoder::start(&root(), &fixture.0, &settings()).unwrap();
        let cancellation = Cancellation::default();
        assert!(decoder.next(&cancellation).unwrap().is_some());
        thread::sleep(Duration::from_millis(100));
        cancellation.cancel();
        assert!(matches!(
            decoder.next(&cancellation),
            Err(AnalysisError::Cancelled)
        ));
        let started = std::time::Instant::now();
        decoder.stop_and_join();
        assert!(decoder.child.try_wait().unwrap().is_some());
        drop(decoder);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn display_rotation_and_public_pipeline_are_wired() {
        let fixture = Fixture::create();
        let rotated = Fixture(fixture.0.with_extension("rotated.mp4"));
        let mut command = Command::new(resolve_ffmpeg_binary(&root()).unwrap());
        configure_ffmpeg_command(&mut command);
        let output = command
            .args(["-v", "error", "-y", "-display_rotation", "90", "-i"])
            .arg(&fixture.0)
            .args(["-c", "copy"])
            .arg(&rotated.0)
            .output()
            .unwrap();
        assert!(output.status.success());
        let cancellation = Cancellation::default();
        let mut decoder = Decoder::start(&root(), &rotated.0, &settings()).unwrap();
        let first = decoder.next(&cancellation).unwrap().unwrap();
        assert_eq!((first.width, first.height), (64, 96));
        drop(decoder);
        let mut count = 0;
        super::super::analyze_video(&root(), &rotated.0, settings(), &cancellation, |interval| {
            assert!(interval.end_seconds > interval.start_seconds);
            count += 1;
            Ok(())
        })
        .unwrap();
        assert!(count > 10);
        let result =
            super::super::analyze_video(&root(), &rotated.0, settings(), &cancellation, |_| {
                cancellation.cancel();
                Ok(())
            });
        assert!(matches!(result, Err(AnalysisError::Cancelled)));
        let result = super::super::analyze_video(
            &root(),
            &rotated.0,
            settings(),
            &Cancellation::default(),
            |_| Err(AnalysisError::Decode("sink stopped".into())),
        );
        assert!(matches!(result, Err(AnalysisError::Decode(message)) if message == "sink stopped"));
    }

    #[test]
    fn missing_video_is_an_error_and_cleanup_finishes() {
        let mut decoder = Decoder::start(
            &root(),
            Path::new("ovrley-missing-motion-input.mp4"),
            &settings(),
        )
        .unwrap();
        assert!(matches!(
            decoder.next(&Cancellation::default()),
            Err(AnalysisError::Decode(_))
        ));
    }

    #[test]
    fn parses_actual_pts_without_inventing_frame_rate() {
        let frame = parse_showinfo(
            "[Parsed_showinfo_4 @ 0] n:  2 pts: 9001 pts_time:1.500167 fmt:gray s:640x360 i:P",
        )
        .unwrap()
        .unwrap();
        assert_eq!(frame.ordinal, 2);
        assert_eq!(frame.seconds, 1.500167);
        assert_eq!((frame.width, frame.height), (640, 360));
        assert!(
            parse_showinfo("[Parsed_showinfo_4 @ 0] n: 2 pts_time:N/A fmt:gray s:640x360").is_err()
        );
        assert!(
            parse_showinfo("[Parsed_showinfo_4 @ 0] config in time_base: 1/90000")
                .unwrap()
                .is_none()
        );
    }
}
