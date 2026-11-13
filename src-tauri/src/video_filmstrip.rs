//! Filmstrips of source videos for the timeline: up to 24 keyframes of one
//! stretch of a video, decoded in a single FFmpeg process and kept as a JPEG
//! sprite (frames side by side) in the disk cache (`filmstrip_cache`).
//!
//! Every video has a base strip of at most 24 frames across its whole length.
//! Zoomed in, the timeline asks for zoom windows: on level `k` frames are
//! `10 s × 2^k` apart and window `w` holds the 24 frames from
//! `w × 24 × 10 s × 2^k` on, so the finest frames are 10 s apart. Each frame is
//! the keyframe at or before the middle of the stretch it stands for.
//!
//! The concat demuxer lists every seek point as a tiny segment of the same
//! file, so one decoder visits them in turn and a hardware decoder sets up its
//! device once. Each segment tags its packets with its sample index, which the
//! decoded frames carry to the `metadata` filter's log.

use crate::filmstrip_cache::{FilmstripCache, FilmstripCacheUsage};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, RgbImage};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

/// Most frames in one strip.
const FRAMES_PER_STRIP: u32 = 24;
/// Closest spacing of frames, on the finest zoom level.
const FINEST_SPACING_SECONDS: f64 = 10.0;
/// Deepest zoom level: frames about 7.6 days apart, past any video.
const MAX_ZOOM_LEVEL: u32 = 16;
/// Longest video a strip is planned for.
const MAX_DURATION_SECONDS: f64 = 7.0 * 24.0 * 3600.0;
/// Largest edge of one filmstrip frame, in pixels.
const MAX_FRAME_EDGE: u32 = 512;
/// Version of the cached strips' sampling and encoding: bump it to miss every older strip.
const CACHE_FORMAT_VERSION: u32 = 1;
/// Latest seek point of the first sample. Some cameras open with a pre-roll group of
/// pictures that the edit list hides, so no shown keyframe sits at or before 0; the
/// next keyframe follows within a second.
const FIRST_SAMPLE_SEEK_SECONDS: f64 = 1.0;
/// Length of each segment after its seek point: the segment then holds the keyframe
/// at or before the point and ends at the next packet past it.
const SEGMENT_SECONDS: f64 = 0.001;
const JPEG_QUALITY: u8 = 80;
const SAMPLE_TAG: &str = "sample";
const BYTES_PER_PIXEL: usize = 3;

/// Hardware decoder chosen from the startup codec detection. Each one is a hardware
/// acceleration of FFmpeg's own decoders, which honour `-skip_frame nokey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FilmstripHwaccel {
    Cuda,
    Vaapi,
    D3d11va,
    Videotoolbox,
}

impl FilmstripHwaccel {
    fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Vaapi => "vaapi",
            Self::D3d11va => "d3d11va",
            Self::Videotoolbox => "videotoolbox",
        }
    }
}

/// Which stretch of a video a strip covers: the whole video, or one window of a zoom level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum FilmstripWindow {
    Base,
    Zoom { level: u32, window: u32 },
}

/// Frames of a strip: sample `i` stands for the stretch from
/// `start + i × spacing` to `start + (i + 1) × spacing`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SamplePlan {
    start_seconds: f64,
    spacing_seconds: f64,
    sample_count: u32,
}

/// Spacing of the base strip: at most 24 frames, never closer than 10 s.
fn base_spacing_seconds(duration_seconds: f64) -> f64 {
    FINEST_SPACING_SECONDS.max(duration_seconds / f64::from(FRAMES_PER_STRIP))
}

/// Frame spacing on a zoom level.
fn zoom_spacing_seconds(level: u32) -> f64 {
    FINEST_SPACING_SECONDS * f64::from(1u32 << level)
}

/// The frames of a strip. A zoom window must space its frames closer than the
/// base strip and start inside the video.
fn plan_samples(duration_seconds: f64, window: FilmstripWindow) -> Result<SamplePlan, String> {
    if !duration_seconds.is_finite()
        || duration_seconds <= 0.0
        || duration_seconds > MAX_DURATION_SECONDS
    {
        return Err(format!(
            "Invalid filmstrip video duration: {duration_seconds}"
        ));
    }
    let (start_seconds, spacing_seconds) = match window {
        FilmstripWindow::Base => (0.0, base_spacing_seconds(duration_seconds)),
        FilmstripWindow::Zoom { level, window } => {
            if level > MAX_ZOOM_LEVEL {
                return Err(format!(
                    "Filmstrip zoom level {level} exceeds {MAX_ZOOM_LEVEL}"
                ));
            }
            let spacing = zoom_spacing_seconds(level);
            if spacing >= base_spacing_seconds(duration_seconds) {
                return Err(format!(
                    "Filmstrip zoom level {level} is no finer than the base strip of a {duration_seconds} s video"
                ));
            }
            let start = f64::from(window) * f64::from(FRAMES_PER_STRIP) * spacing;
            if start >= duration_seconds {
                return Err(format!(
                    "Filmstrip zoom window {window} of level {level} starts past the end of a {duration_seconds} s video"
                ));
            }
            (start, spacing)
        }
    };
    let remaining = ((duration_seconds - start_seconds) / spacing_seconds).ceil() as u32;
    Ok(SamplePlan {
        start_seconds,
        spacing_seconds,
        sample_count: remaining.clamp(1, FRAMES_PER_STRIP),
    })
}

/// Seek point of every sample: the middle of its stretch, cut off at the end of
/// the video. The first sample of the video waits out a hidden pre-roll.
fn sample_seek_seconds(plan: &SamplePlan, duration_seconds: f64) -> Vec<f64> {
    (0..plan.sample_count)
        .map(|index| {
            let from = plan.start_seconds + f64::from(index) * plan.spacing_seconds;
            let to = (from + plan.spacing_seconds).min(duration_seconds);
            let middle = (from + to) / 2.0;
            if from == 0.0 {
                middle.max(FIRST_SAMPLE_SEEK_SECONDS.min(to * 0.9))
            } else {
                middle
            }
        })
        .collect()
}

/// Validated filmstrip request: the strip `window` of a `duration_seconds` long
/// video, each frame scaled to `frame_width` × `frame_height`.
#[derive(Debug, Clone, PartialEq)]
struct FilmstripRequest {
    video_path: PathBuf,
    duration_seconds: f64,
    window: FilmstripWindow,
    plan: SamplePlan,
    frame_width: u32,
    frame_height: u32,
}

impl FilmstripRequest {
    fn new(
        video_path: PathBuf,
        duration_seconds: f64,
        window: FilmstripWindow,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, String> {
        if !video_path.is_absolute() {
            return Err("Filmstrip video path must be absolute".into());
        }
        for edge in [frame_width, frame_height] {
            if edge == 0 || edge > MAX_FRAME_EDGE {
                return Err(format!(
                    "Filmstrip frame size must be between 1 and {MAX_FRAME_EDGE} pixels: {frame_width}×{frame_height}"
                ));
            }
        }
        Ok(Self {
            plan: plan_samples(duration_seconds, window)?,
            video_path,
            duration_seconds,
            window,
            frame_width,
            frame_height,
        })
    }

    fn frame_len(&self) -> usize {
        self.frame_width as usize * self.frame_height as usize * BYTES_PER_PIXEL
    }
}

/// A filmstrip: a base64 JPEG with its frames side by side, sample `i` standing
/// for the stretch from `start + i × spacing` to `start + (i + 1) × spacing`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Filmstrip {
    pub sprite_jpeg_base64: String,
    pub start_seconds: f64,
    pub spacing_seconds: f64,
    pub sample_count: u32,
}

impl Filmstrip {
    fn of(request: &FilmstripRequest, jpeg: &[u8]) -> Self {
        Self {
            sprite_jpeg_base64: BASE64_STANDARD.encode(jpeg),
            start_seconds: request.plan.start_seconds,
            spacing_seconds: request.plan.spacing_seconds,
            sample_count: request.plan.sample_count,
        }
    }
}

/// 64-bit FNV-1a: a hash that stays the same across builds, for cache names.
fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Cache name of a strip: the source file (path, size and modified time, so a
/// replaced file misses), the cache format, the window, the sampling and the
/// frame size. `None` when the file cannot be read.
fn cache_name(request: &FilmstripRequest) -> Option<String> {
    let meta = std::fs::metadata(&request.video_path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    let window = match request.window {
        FilmstripWindow::Base => "base".to_string(),
        FilmstripWindow::Zoom { level, window } => format!("z{level}-{window}"),
    };
    let path_hash = stable_hash(request.video_path.to_string_lossy().as_bytes());
    Some(format!(
        "{path_hash:016x}-{:x}-{modified:x}.v{CACHE_FORMAT_VERSION}.{window}.{}x{}ms.{}x{}",
        meta.len(),
        request.plan.sample_count,
        (request.plan.spacing_seconds * 1000.0).round() as u64,
        request.frame_width,
        request.frame_height,
    ))
}

/// Quotes a path for a concat list line. Inside single quotes everything is literal,
/// so a quote closes the string, adds an escaped quote and opens it again.
fn quote_concat_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or_else(|| {
        format!(
            "Filmstrip video path is not valid UTF-8: {}",
            path.display()
        )
    })?;
    if text.contains(['\n', '\r']) {
        return Err(format!(
            "Filmstrip video path contains a line break: {text}"
        ));
    }
    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

/// The concat list visiting every seek point: one tagged segment per sample.
///
/// `seek_streams_individually 0` makes the MP4/MOV demuxer start every stream at the
/// video keyframe. Seeking streams individually starts the audio up to a chunk
/// earlier in the file, and audio packets past the outpoint end the segment before
/// its keyframe is read. Other demuxers ignore the option.
fn concat_list(video_path: &Path, seeks: &[f64]) -> Result<String, String> {
    let quoted = quote_concat_path(video_path)?;
    let mut list = String::from("ffconcat version 1.0\n");
    for (index, seek) in seeks.iter().enumerate() {
        let _ = writeln!(list, "file {quoted}");
        let _ = writeln!(list, "option seek_streams_individually 0");
        let _ = writeln!(list, "file_packet_meta {SAMPLE_TAG} {index}");
        let _ = writeln!(list, "inpoint {seek:.6}");
        let _ = writeln!(list, "outpoint {:.6}", seek + SEGMENT_SECONDS);
    }
    Ok(list)
}

/// FFmpeg arguments decoding the keyframes of a concat list as raw RGB frames of a
/// fixed size on stdout, logging the sample tag of each frame. With a hardware
/// decoder, FFmpeg downloads each decoded frame itself, so scaling (which washes out
/// 10-bit colour on the GPU) and autorotation run on the CPU.
///
/// A segment holds the packets from its keyframe up to its seek point, up to a group
/// of pictures; the input bitstream filter drops all but the keyframes before they
/// reach the decoder, which would otherwise pass each through its threads only to
/// skip it.
///
/// Both the filter and the map name the first video stream that is not a cover
/// image (`V:0`). DJI files carry an MJPEG cover; the filter dropping its packet
/// aborts FFmpeg on an internal assertion.
fn filmstrip_args(
    list_path: &Path,
    request: &FilmstripRequest,
    hwaccel: Option<FilmstripHwaccel>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info"]
        .into_iter()
        .map(OsString::from)
        .collect();
    if let Some(hwaccel) = hwaccel {
        args.push("-hwaccel".into());
        args.push(hwaccel.ffmpeg_name().into());
    }
    // Segment timestamps overlap (each keyframe sits before its segment's start), so
    // frame `N` is moved to second `N` for the muxer, which requires increasing
    // timestamps at the output's frame-rate time base.
    let filter = format!(
        "setpts=N/TB,scale={}:{}:flags=fast_bilinear,metadata=mode=print:key={SAMPLE_TAG}",
        request.frame_width, request.frame_height
    );
    for arg in [
        "-skip_frame",
        "nokey",
        "-bsf:V:0",
        "noise=drop=not(key)",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ] {
        args.push(arg.into());
    }
    args.push(list_path.into());
    for arg in [
        "-map",
        "0:V:0",
        "-vf",
        &filter,
        "-fps_mode",
        "passthrough",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ] {
        args.push(arg.into());
    }
    args
}

/// Sample index of each decoded frame, in output order, from the `metadata` filter's
/// log lines (`[Parsed_metadata_1 @ 0x…] sample=3`).
fn parse_sample_tags(stderr: &str) -> Result<Vec<u32>, String> {
    let prefix = format!("] {SAMPLE_TAG}=");
    stderr
        .lines()
        .filter(|line| line.starts_with("[Parsed_metadata"))
        .filter_map(|line| line.split_once(prefix.as_str()).map(|(_, value)| value))
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("Invalid filmstrip sample tag: {value}"))
        })
        .collect()
}

/// Raw frames of one FFmpeg run with the sample index of each.
#[derive(Debug)]
struct DecodedFrames {
    pixels: Vec<u8>,
    samples: Vec<u32>,
}

/// Runs one decode. `None` when FFmpeg fails or decodes no frame; the log says why.
fn decode_frames(
    ffmpeg: &Path,
    list_path: &Path,
    request: &FilmstripRequest,
    hwaccel: Option<FilmstripHwaccel>,
) -> Option<DecodedFrames> {
    let mut command = Command::new(ffmpeg);
    ovrley_core::encode::ffmpeg::binary::configure_ffmpeg_command(&mut command);
    let decoder = hwaccel.map_or("software", FilmstripHwaccel::ffmpeg_name);
    let output = match command
        .args(filmstrip_args(list_path, request, hwaccel))
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            log::warn!("Filmstrip FFmpeg could not start: {error}");
            return None;
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let failure = |reason: String| {
        let tail: Vec<&str> = stderr.lines().rev().take(4).collect();
        log::warn!(
            "Filmstrip decode ({decoder}) of {} failed: {reason}; FFmpeg: {}",
            request.video_path.display(),
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        );
        None
    };
    if !output.status.success() {
        return failure(format!("exit status {}", output.status));
    }
    let samples = match parse_sample_tags(&stderr) {
        Ok(samples) => samples,
        Err(error) => return failure(error),
    };
    let frame_len = request.frame_len();
    if output.stdout.len() != samples.len() * frame_len {
        return failure(format!(
            "{} bytes of frames for {} sample tags",
            output.stdout.len(),
            samples.len()
        ));
    }
    if samples.is_empty() {
        return failure("no frame decoded".into());
    }
    if let Some(sample) = samples
        .iter()
        .find(|sample| **sample >= request.plan.sample_count)
    {
        return failure(format!("sample tag {sample} out of range"));
    }
    Some(DecodedFrames {
        pixels: output.stdout,
        samples,
    })
}

/// Frame shown for each sample: the first frame decoded for it, or for a sample whose
/// segment held no keyframe, the frame of the nearest sample (the earlier one on a tie).
fn frame_per_sample(samples: &[u32], sample_count: u32) -> Vec<usize> {
    let mut first_frame: Vec<Option<usize>> = vec![None; sample_count as usize];
    for (frame, sample) in samples.iter().enumerate() {
        first_frame[*sample as usize].get_or_insert(frame);
    }
    let decoded: Vec<(usize, usize)> = first_frame
        .iter()
        .enumerate()
        .filter_map(|(sample, frame)| frame.map(|frame| (sample, frame)))
        .collect();
    (0..sample_count as usize)
        .map(|sample| {
            decoded
                .iter()
                .min_by_key(|(decoded_sample, _)| {
                    (decoded_sample.abs_diff(sample), *decoded_sample)
                })
                .map(|(_, frame)| *frame)
                .expect("a decode yields at least one frame")
        })
        .collect()
}

/// The strip's sprite: its frames side by side, in sample order.
fn compose_sprite(decoded: &DecodedFrames, request: &FilmstripRequest) -> RgbImage {
    let (width, height) = (request.frame_width, request.frame_height);
    let mut sprite = RgbImage::new(request.plan.sample_count * width, height);
    let frame_len = request.frame_len();
    let row_len = width as usize * BYTES_PER_PIXEL;
    let sprite_row_len = sprite.width() as usize * BYTES_PER_PIXEL;
    let buffer: &mut [u8] = &mut sprite;
    for (sample, frame) in frame_per_sample(&decoded.samples, request.plan.sample_count)
        .into_iter()
        .enumerate()
    {
        let pixels = &decoded.pixels[frame * frame_len..(frame + 1) * frame_len];
        for (line, source) in pixels.chunks_exact(row_len).enumerate() {
            let start = line * sprite_row_len + sample * row_len;
            buffer[start..start + row_len].copy_from_slice(source);
        }
    }
    sprite
}

fn encode_jpeg(sprite: &RgbImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .encode(
            sprite.as_raw(),
            sprite.width(),
            sprite.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("Filmstrip sprite encoding failed: {error}"))?;
    Ok(bytes)
}

/// Decodes the sprite of a strip as a JPEG, with the hardware decoder when one is
/// given and again in software when that run fails. `None` when FFmpeg decodes no frame.
fn generate_sprite(
    ffmpeg: &Path,
    request: &FilmstripRequest,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Vec<u8>>, String> {
    let seeks = sample_seek_seconds(&request.plan, request.duration_seconds);
    let list = concat_list(&request.video_path, &seeks)?;
    let list_path = std::env::temp_dir().join(format!(
        "ovrley-filmstrip-{}.ffconcat",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&list_path, list)
        .map_err(|error| format!("Filmstrip list could not be written: {error}"))?;
    let decoded = hwaccel
        .and_then(|hwaccel| decode_frames(ffmpeg, &list_path, request, Some(hwaccel)))
        .or_else(|| decode_frames(ffmpeg, &list_path, request, None));
    let _ = std::fs::remove_file(&list_path);
    decoded
        .map(|decoded| encode_jpeg(&compose_sprite(&decoded, request)))
        .transpose()
}

/// The strip from the disk cache, or `None` on a miss.
fn cached_filmstrip(
    cache: &FilmstripCache,
    request: &FilmstripRequest,
) -> Result<Option<Filmstrip>, String> {
    let Some(name) = cache_name(request) else {
        return Ok(None);
    };
    Ok(cache.read(&name)?.map(|jpeg| Filmstrip::of(request, &jpeg)))
}

/// The strip from the disk cache, or decoded and stored in it. `None` when FFmpeg
/// decodes no frame. A strip that cannot be stored is still returned.
fn filmstrip(
    cache: &FilmstripCache,
    ffmpeg: &Path,
    request: &FilmstripRequest,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Filmstrip>, String> {
    if let Some(filmstrip) = cached_filmstrip(cache, request)? {
        return Ok(Some(filmstrip));
    }
    let Some(jpeg) = generate_sprite(ffmpeg, request, hwaccel)? else {
        return Ok(None);
    };
    match cache_name(request) {
        Some(name) => {
            if let Err(error) = cache.write(&name, &jpeg) {
                log::warn!(
                    "Filmstrip of {} not cached: {error}",
                    request.video_path.display()
                );
            }
        }
        None => log::warn!(
            "Filmstrip of {} not cached: the file cannot be read",
            request.video_path.display()
        ),
    }
    Ok(Some(Filmstrip::of(request, &jpeg)))
}

/// The cached strip `window` of a source video, without decoding anything:
/// `None` on a cache miss. Cheap enough to run outside the decode queue.
#[tauri::command]
pub(crate) async fn backend_cached_video_filmstrip(
    cache: tauri::State<'_, FilmstripCache>,
    path: String,
    duration_seconds: f64,
    window: FilmstripWindow,
    frame_width: u32,
    frame_height: u32,
) -> Result<Option<Filmstrip>, String> {
    let request = FilmstripRequest::new(
        PathBuf::from(path),
        duration_seconds,
        window,
        frame_width,
        frame_height,
    )?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cached_filmstrip(&cache, &request))
        .await
        .map_err(|error| format!("Video filmstrip task failed: {error}"))?
}

/// The strip `window` of a source video, from the disk cache or decoded with
/// `hwaccel` (or in software) and cached. `None` when FFmpeg cannot decode a
/// frame of it; an FFmpeg that cannot be found is an error.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn backend_video_filmstrip(
    app: tauri::AppHandle,
    cache: tauri::State<'_, FilmstripCache>,
    path: String,
    duration_seconds: f64,
    window: FilmstripWindow,
    frame_width: u32,
    frame_height: u32,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Filmstrip>, String> {
    let request = FilmstripRequest::new(
        PathBuf::from(path),
        duration_seconds,
        window,
        frame_width,
        frame_height,
    )?;
    let paths = crate::runtime_paths::app_paths(&app)?;
    let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root)
        .map_err(|error| error.to_string())?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || filmstrip(&cache, &ffmpeg, &request, hwaccel))
        .await
        .map_err(|error| format!("Video filmstrip task failed: {error}"))?
}

/// The filmstrip cache's current size and limit.
#[tauri::command]
pub(crate) async fn backend_get_filmstrip_cache(
    cache: tauri::State<'_, FilmstripCache>,
) -> Result<FilmstripCacheUsage, String> {
    cache.usage()
}

/// Saves and applies a new filmstrip cache limit, evicting down to it.
#[tauri::command]
pub(crate) async fn backend_set_filmstrip_cache_limit(
    app: tauri::AppHandle,
    cache: tauri::State<'_, FilmstripCache>,
    limit_bytes: u64,
) -> Result<FilmstripCacheUsage, String> {
    let cache = cache.inner().clone();
    let usage = tauri::async_runtime::spawn_blocking(move || cache.set_limit(limit_bytes))
        .await
        .map_err(|error| error.to_string())??;
    crate::filmstrip_cache::persist_filmstrip_cache_limit(&app, limit_bytes)?;
    Ok(usage)
}

/// Deletes every cached filmstrip.
#[tauri::command]
pub(crate) async fn backend_clear_filmstrip_cache(
    cache: tauri::State<'_, FilmstripCache>,
) -> Result<FilmstripCacheUsage, String> {
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cache.clear())
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute(path: &str) -> PathBuf {
        std::env::temp_dir().join(path)
    }

    fn base_request(
        duration_seconds: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> FilmstripRequest {
        FilmstripRequest::new(
            absolute("ride.mp4"),
            duration_seconds,
            FilmstripWindow::Base,
            frame_width,
            frame_height,
        )
        .unwrap()
    }

    fn plan(start_seconds: f64, spacing_seconds: f64, sample_count: u32) -> SamplePlan {
        SamplePlan {
            start_seconds,
            spacing_seconds,
            sample_count,
        }
    }

    #[test]
    fn request_rejects_malformed_input() {
        let path = absolute("ride.mp4");
        let base = FilmstripWindow::Base;
        assert!(FilmstripRequest::new(PathBuf::from("ride.mp4"), 60.0, base, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 0.0, base, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), f64::NAN, base, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 60.0, base, 0, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 60.0, base, 4, MAX_FRAME_EDGE + 1).is_err());
        assert!(FilmstripRequest::new(path, 60.0, base, 171, 96).is_ok());
    }

    #[test]
    fn the_base_strip_holds_at_most_24_frames_never_closer_than_10_s() {
        // A 2 h video: 24 frames 5 min apart.
        assert_eq!(
            plan_samples(7200.0, FilmstripWindow::Base).unwrap(),
            plan(0.0, 300.0, 24)
        );
        // A 237 s video: 10 s spacing makes 24 frames, the last one standing for 7 s.
        assert_eq!(
            plan_samples(237.0, FilmstripWindow::Base).unwrap(),
            plan(0.0, 10.0, 24)
        );
        // A short video gets fewer frames, at least one.
        assert_eq!(
            plan_samples(35.0, FilmstripWindow::Base).unwrap(),
            plan(0.0, 10.0, 4)
        );
        assert_eq!(
            plan_samples(3.0, FilmstripWindow::Base).unwrap(),
            plan(0.0, 10.0, 1)
        );
    }

    #[test]
    fn zoom_windows_lie_on_a_seconds_grid_down_to_10_s() {
        let zoom = |level, window| FilmstripWindow::Zoom { level, window };
        // Level 0: frames 10 s apart, 240 s per window.
        assert_eq!(
            plan_samples(7200.0, zoom(0, 2)).unwrap(),
            plan(480.0, 10.0, 24)
        );
        // Level 3: frames 80 s apart, 1920 s per window; the last window holds what is left.
        assert_eq!(
            plan_samples(7200.0, zoom(3, 3)).unwrap(),
            plan(5760.0, 80.0, 18)
        );
        // A level no finer than the base strip, or a window past the end, is refused.
        assert!(plan_samples(7200.0, zoom(5, 0)).is_err());
        assert!(plan_samples(237.0, zoom(0, 0)).is_err());
        assert!(plan_samples(7200.0, zoom(0, 30)).is_err());
        assert!(plan_samples(7200.0, zoom(MAX_ZOOM_LEVEL + 1, 0)).is_err());
    }

    #[test]
    fn windows_deserialize_from_the_frontend() {
        let base: FilmstripWindow = serde_json::from_str(r#"{"kind":"base"}"#).unwrap();
        let zoom: FilmstripWindow =
            serde_json::from_str(r#"{"kind":"zoom","level":2,"window":5}"#).unwrap();
        assert_eq!(base, FilmstripWindow::Base);
        assert_eq!(
            zoom,
            FilmstripWindow::Zoom {
                level: 2,
                window: 5
            }
        );
    }

    #[test]
    fn each_sample_seeks_to_the_middle_of_its_stretch() {
        // The first waits out a hidden pre-roll only when its middle is earlier.
        assert_eq!(
            sample_seek_seconds(&plan(0.0, 10.0, 3), 25.0),
            vec![5.0, 15.0, 22.5]
        );
        assert_eq!(sample_seek_seconds(&plan(0.0, 10.0, 1), 1.5), vec![1.0]);
        assert_eq!(
            sample_seek_seconds(&plan(480.0, 10.0, 2), 7200.0),
            vec![485.0, 495.0]
        );
    }

    #[test]
    fn concat_list_tags_one_tiny_segment_per_sample() {
        let list = concat_list(Path::new("/videos/ride.mp4"), &[1.0, 20.0]).unwrap();
        assert_eq!(
            list,
            "ffconcat version 1.0\n\
             file '/videos/ride.mp4'\n\
             option seek_streams_individually 0\n\
             file_packet_meta sample 0\n\
             inpoint 1.000000\n\
             outpoint 1.001000\n\
             file '/videos/ride.mp4'\n\
             option seek_streams_individually 0\n\
             file_packet_meta sample 1\n\
             inpoint 20.000000\n\
             outpoint 20.001000\n"
        );
    }

    #[test]
    fn concat_paths_escape_quotes_and_keep_backslashes() {
        assert_eq!(
            quote_concat_path(Path::new("/a/it's.mp4")).unwrap(),
            r"'/a/it'\''s.mp4'"
        );
        assert_eq!(
            quote_concat_path(Path::new(r"C:\clips\ride.mp4")).unwrap(),
            r"'C:\clips\ride.mp4'"
        );
        assert!(quote_concat_path(Path::new("/a/line\nbreak.mp4")).is_err());
    }

    #[test]
    fn args_use_the_hardware_decoder_only_when_given() {
        let request = base_request(237.0, 170, 96);
        let list = Path::new("/tmp/list.ffconcat");
        let software: Vec<String> = filmstrip_args(list, &request, None)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        let cuda: Vec<String> = filmstrip_args(list, &request, Some(FilmstripHwaccel::Cuda))
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();

        assert!(!software.contains(&"-hwaccel".to_string()));
        let hwaccel = cuda.iter().position(|arg| arg == "-hwaccel").unwrap();
        assert_eq!(cuda[hwaccel + 1], "cuda");
        // The decoder option comes before the input it applies to.
        assert!(hwaccel < cuda.iter().position(|arg| arg == "-i").unwrap());
        assert_eq!(cuda.len(), software.len() + 2);
        let filter = software.iter().position(|arg| arg == "-vf").unwrap();
        assert_eq!(
            software[filter + 1],
            "setpts=N/TB,scale=170:96:flags=fast_bilinear,metadata=mode=print:key=sample"
        );
        // Non-keyframe packets are dropped on the input, before the decoder.
        let bsf = software.iter().position(|arg| arg == "-bsf:V:0").unwrap();
        assert_eq!(software[bsf + 1], "noise=drop=not(key)");
        assert!(bsf < software.iter().position(|arg| arg == "-i").unwrap());
        assert!(software.ends_with(&["-f".into(), "rawvideo".into(), "pipe:1".into()]));
    }

    #[test]
    fn hwaccel_names_deserialize_from_the_frontend() {
        let names = ["cuda", "vaapi", "d3d11va", "videotoolbox"];
        let parsed: Vec<FilmstripHwaccel> = names
            .iter()
            .map(|name| serde_json::from_str(&format!("\"{name}\"")).unwrap())
            .collect();
        assert_eq!(
            parsed
                .iter()
                .map(|hwaccel| hwaccel.ffmpeg_name())
                .collect::<Vec<_>>(),
            names
        );
        assert!(serde_json::from_str::<FilmstripHwaccel>("\"qsv\"").is_err());
    }

    #[test]
    fn sample_tags_are_read_from_metadata_log_lines() {
        let stderr = "\
[in#0/concat @ 0x1] Unused options for '/v/ride.mkv'.
[Parsed_metadata_1 @ 0x7f] frame:0    pts:0       pts_time:0
[Parsed_metadata_1 @ 0x7f] sample=0
[Parsed_metadata_1 @ 0x7f] frame:1    pts:-55860  pts_time:-0.931
[Parsed_metadata_1 @ 0x7f] sample=12
";
        assert_eq!(parse_sample_tags(stderr).unwrap(), vec![0, 12]);
        assert!(parse_sample_tags("[Parsed_metadata_1 @ 0x7f] sample=x").is_err());
    }

    #[test]
    fn each_sample_shows_its_first_frame_or_the_nearest_decoded_sample() {
        // Sample 0 and 3 decoded nothing; sample 1 decoded two keyframes.
        assert_eq!(frame_per_sample(&[1, 1, 2, 4], 6), vec![0, 0, 2, 2, 3, 3]);
        // A tie takes the earlier sample.
        assert_eq!(frame_per_sample(&[0, 2], 3), vec![0, 0, 1]);
    }

    #[test]
    fn sprite_places_the_frames_side_by_side() {
        // A 25 s video: three samples of 2×1 frames.
        let request = base_request(25.0, 2, 1);
        // Frame 0 is all 10s, frame 1 all 20s; sample 1 has no frame.
        let decoded = DecodedFrames {
            pixels: [vec![10; 6], vec![20; 6]].concat(),
            samples: vec![0, 2],
        };
        let sprite = compose_sprite(&decoded, &request);

        assert_eq!((sprite.width(), sprite.height()), (6, 1));
        assert_eq!(sprite.get_pixel(0, 0).0, [10, 10, 10]);
        // Sample 1 repeats sample 0, the earlier of its equally near neighbours.
        assert_eq!(sprite.get_pixel(2, 0).0, [10, 10, 10]);
        assert_eq!(sprite.get_pixel(5, 0).0, [20, 20, 20]);
    }

    #[test]
    fn cache_names_follow_the_file_the_window_and_the_frame_size() {
        let dir =
            std::env::temp_dir().join(format!("ovrley-filmstrip-name-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("ride.mp4");
        std::fs::write(&video, b"one").unwrap();
        let request = |window, height| {
            FilmstripRequest::new(video.clone(), 7200.0, window, 171, height).unwrap()
        };
        let base = cache_name(&request(FilmstripWindow::Base, 96)).unwrap();
        let zoom = cache_name(&request(
            FilmstripWindow::Zoom {
                level: 1,
                window: 3,
            },
            96,
        ))
        .unwrap();
        let taller = cache_name(&request(FilmstripWindow::Base, 120)).unwrap();
        std::fs::write(&video, b"longer").unwrap();
        let replaced = cache_name(&request(FilmstripWindow::Base, 96)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(base.contains(".v1.base.24x300000ms.171x96"), "{base}");
        assert!(zoom.contains(".z1-3.24x20000ms."), "{zoom}");
        let distinct: std::collections::HashSet<_> =
            [&base, &zoom, &taller, &replaced].into_iter().collect();
        assert_eq!(distinct.len(), 4);
        assert!(cache_name(&request(FilmstripWindow::Base, 96)).is_none());
    }

    fn repo_ffmpeg() -> PathBuf {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(repo_root).unwrap()
    }

    /// A 6 s 160×90 video at 25 fps with a keyframe every second, an audio track and an
    /// MJPEG cover image, as DJI cameras write.
    fn write_test_video(ffmpeg: &Path, path: &Path) {
        let mut command = Command::new(ffmpeg);
        let status = command
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=6"])
            .args(["-f", "lavfi", "-i", "sine=f=440:r=48000:d=6"])
            .args(["-f", "lavfi", "-i", "color=c=red:s=64x36:d=0.04"])
            .args(["-map", "0:v", "-map", "1:a", "-map", "2:v"])
            .args([
                "-c:v:0", "libx264", "-g", "25", "-pix_fmt", "yuv420p", "-c:a", "aac",
            ])
            .args(["-c:v:1", "mjpeg", "-disposition:v:1", "attached_pic"])
            .arg(path)
            .stdin(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn decodes_a_video_into_a_cached_sprite_and_falls_back_to_software() {
        let ffmpeg = repo_ffmpeg();
        let dir =
            std::env::temp_dir().join(format!("ovrley-filmstrip-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("it's a clip.mp4");
        write_test_video(&ffmpeg, &video);
        let cache = FilmstripCache::open(
            dir.join("cache"),
            crate::filmstrip_cache::DEFAULT_FILMSTRIP_CACHE_LIMIT,
        )
        .unwrap();
        let request = FilmstripRequest::new(video, 6.0, FilmstripWindow::Base, 32, 18).unwrap();
        // A decoder this platform lacks fails, and the software run takes over.
        let unavailable = if cfg!(target_os = "macos") {
            FilmstripHwaccel::D3d11va
        } else {
            FilmstripHwaccel::Videotoolbox
        };

        assert_eq!(cached_filmstrip(&cache, &request).unwrap(), None);
        let decoded = filmstrip(&cache, &ffmpeg, &request, Some(unavailable))
            .unwrap()
            .unwrap();
        // The second request is served from the cache without FFmpeg.
        let cached = cached_filmstrip(&cache, &request).unwrap().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(cached, decoded);
        assert_eq!(
            (
                decoded.start_seconds,
                decoded.spacing_seconds,
                decoded.sample_count
            ),
            (0.0, 10.0, 1)
        );
        let jpeg = BASE64_STANDARD.decode(decoded.sprite_jpeg_base64).unwrap();
        let sprite = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((sprite.width(), sprite.height()), (32, 18));
    }

    #[test]
    fn a_file_without_video_yields_no_filmstrip() {
        let ffmpeg = repo_ffmpeg();
        let path = std::env::temp_dir().join(format!(
            "ovrley-filmstrip-empty-{}.mp4",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, b"not a video").unwrap();
        let request =
            FilmstripRequest::new(path.clone(), 6.0, FilmstripWindow::Base, 32, 18).unwrap();

        let sprite = generate_sprite(&ffmpeg, &request, None).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(sprite, None);
    }
}
