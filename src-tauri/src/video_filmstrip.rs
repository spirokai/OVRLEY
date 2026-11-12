//! Filmstrips of source videos for the timeline: one frame per sample interval,
//! decoded in a single FFmpeg process and returned as a JPEG sprite sheet. Made
//! after import and project load and never stored.
//!
//! Each sample is the keyframe at or before its seek point. The concat demuxer
//! lists every seek point as a tiny segment of the same file, so one decoder
//! visits them in turn and a hardware decoder sets up its device once. Each
//! segment tags its packets with its sample index, which the decoded frames
//! carry to the `metadata` filter's log.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, RgbImage};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Most samples one filmstrip may hold.
const MAX_SAMPLE_COUNT: u32 = 1000;
/// Largest edge of one filmstrip frame, in pixels.
const MAX_FRAME_EDGE: u32 = 512;
/// Largest sprite sheet, in pixels.
const MAX_SPRITE_PIXELS: u64 = 32 * 1024 * 1024;
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

/// Validated filmstrip request: `sample_count` samples, `interval_seconds` apart,
/// each scaled to `frame_width` × `frame_height`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FilmstripRequest {
    video_path: PathBuf,
    interval_seconds: f64,
    sample_count: u32,
    frame_width: u32,
    frame_height: u32,
}

impl FilmstripRequest {
    pub(crate) fn new(
        video_path: PathBuf,
        interval_seconds: f64,
        sample_count: u32,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, String> {
        if !video_path.is_absolute() {
            return Err("Filmstrip video path must be absolute".into());
        }
        if !interval_seconds.is_finite() || interval_seconds <= 0.0 {
            return Err(format!(
                "Invalid filmstrip sample interval: {interval_seconds}"
            ));
        }
        if sample_count == 0 || sample_count > MAX_SAMPLE_COUNT {
            return Err(format!(
                "Filmstrip sample count must be between 1 and {MAX_SAMPLE_COUNT}: {sample_count}"
            ));
        }
        for edge in [frame_width, frame_height] {
            if edge == 0 || edge > MAX_FRAME_EDGE {
                return Err(format!(
                    "Filmstrip frame size must be between 1 and {MAX_FRAME_EDGE} pixels: {frame_width}×{frame_height}"
                ));
            }
        }
        let (columns, rows) = sprite_grid(sample_count);
        let sprite_pixels = u64::from(columns * frame_width) * u64::from(rows * frame_height);
        if sprite_pixels > MAX_SPRITE_PIXELS {
            return Err(format!(
                "Filmstrip sprite of {sample_count} frames of {frame_width}×{frame_height} exceeds {MAX_SPRITE_PIXELS} pixels"
            ));
        }
        Ok(Self {
            video_path,
            interval_seconds,
            sample_count,
            frame_width,
            frame_height,
        })
    }

    fn frame_len(&self) -> usize {
        self.frame_width as usize * self.frame_height as usize * BYTES_PER_PIXEL
    }
}

/// A filmstrip sprite sheet, a base64 JPEG: sample `i` is the cell at column
/// `i % columns`, row `i / columns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Filmstrip {
    pub sprite_jpeg_base64: String,
    pub columns: u32,
    pub rows: u32,
}

/// Seek point of every sample: sample `i` at `i × interval`, the first one moved
/// past a hidden pre-roll.
fn sample_seek_seconds(interval_seconds: f64, sample_count: u32) -> Vec<f64> {
    (0..sample_count)
        .map(|index| {
            if index == 0 {
                FIRST_SAMPLE_SEEK_SECONDS.min(interval_seconds / 2.0)
            } else {
                f64::from(index) * interval_seconds
            }
        })
        .collect()
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
        .find(|sample| **sample >= request.sample_count)
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

/// Grid of a sprite sheet: as square as possible, filled row by row.
fn sprite_grid(sample_count: u32) -> (u32, u32) {
    let columns = (f64::from(sample_count).sqrt().ceil() as u32).max(1);
    (columns, sample_count.div_ceil(columns))
}

fn compose_sprite(decoded: &DecodedFrames, request: &FilmstripRequest) -> (RgbImage, u32, u32) {
    let (columns, rows) = sprite_grid(request.sample_count);
    let (width, height) = (request.frame_width, request.frame_height);
    let mut sprite = RgbImage::new(columns * width, rows * height);
    let frame_len = request.frame_len();
    let row_len = width as usize * BYTES_PER_PIXEL;
    for (sample, frame) in frame_per_sample(&decoded.samples, request.sample_count)
        .into_iter()
        .enumerate()
    {
        let pixels = &decoded.pixels[frame * frame_len..(frame + 1) * frame_len];
        let (cell_x, cell_y) = (
            sample as u32 % columns * width,
            sample as u32 / columns * height,
        );
        let sprite_width = sprite.width() as usize;
        let buffer: &mut [u8] = &mut sprite;
        for (line, source) in pixels.chunks_exact(row_len).enumerate() {
            let start =
                ((cell_y as usize + line) * sprite_width + cell_x as usize) * BYTES_PER_PIXEL;
            buffer[start..start + row_len].copy_from_slice(source);
        }
    }
    (sprite, columns, rows)
}

fn jpeg_base64(sprite: &RgbImage) -> Result<String, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .encode(
            sprite.as_raw(),
            sprite.width(),
            sprite.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("Filmstrip sprite encoding failed: {error}"))?;
    Ok(BASE64_STANDARD.encode(bytes))
}

/// Decodes the filmstrip of a video, with the hardware decoder when one is given and
/// again in software when that run fails. `None` when FFmpeg decodes no frame.
pub(crate) fn generate_filmstrip(
    ffmpeg: &Path,
    request: &FilmstripRequest,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Filmstrip>, String> {
    let seeks = sample_seek_seconds(request.interval_seconds, request.sample_count);
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
    let Some(decoded) = decoded else {
        return Ok(None);
    };
    let (sprite, columns, rows) = compose_sprite(&decoded, request);
    Ok(Some(Filmstrip {
        sprite_jpeg_base64: jpeg_base64(&sprite)?,
        columns,
        rows,
    }))
}

/// Renders the timeline filmstrip of a source video: `sample_count` frames
/// `interval_seconds` apart, each `frame_width` × `frame_height`. `None` when FFmpeg
/// cannot decode a frame of it; an FFmpeg that cannot be found is an error.
#[tauri::command]
pub(crate) async fn backend_video_filmstrip(
    app: tauri::AppHandle,
    path: String,
    interval_seconds: f64,
    sample_count: u32,
    frame_width: u32,
    frame_height: u32,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Filmstrip>, String> {
    let request = FilmstripRequest::new(
        PathBuf::from(path),
        interval_seconds,
        sample_count,
        frame_width,
        frame_height,
    )?;
    let paths = crate::runtime_paths::app_paths(&app)?;
    let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root)
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || generate_filmstrip(&ffmpeg, &request, hwaccel))
        .await
        .map_err(|error| format!("Video filmstrip task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute(path: &str) -> PathBuf {
        std::env::temp_dir().join(path)
    }

    fn request(sample_count: u32, frame_width: u32, frame_height: u32) -> FilmstripRequest {
        FilmstripRequest::new(
            absolute("ride.mp4"),
            20.0,
            sample_count,
            frame_width,
            frame_height,
        )
        .unwrap()
    }

    #[test]
    fn request_rejects_malformed_input() {
        let path = absolute("ride.mp4");
        assert!(FilmstripRequest::new(PathBuf::from("ride.mp4"), 20.0, 10, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 0.0, 10, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), f64::NAN, 10, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 20.0, 0, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 20.0, MAX_SAMPLE_COUNT + 1, 4, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 20.0, 10, 0, 2).is_err());
        assert!(FilmstripRequest::new(path.clone(), 20.0, 10, 4, MAX_FRAME_EDGE + 1).is_err());
        assert!(FilmstripRequest::new(
            path.clone(),
            20.0,
            MAX_SAMPLE_COUNT,
            MAX_FRAME_EDGE,
            MAX_FRAME_EDGE
        )
        .is_err());
        assert!(FilmstripRequest::new(path, 20.0, MAX_SAMPLE_COUNT, 171, 96).is_ok());
    }

    #[test]
    fn samples_are_an_interval_apart_with_the_first_past_a_hidden_preroll() {
        assert_eq!(sample_seek_seconds(20.0, 4), vec![1.0, 20.0, 40.0, 60.0]);
        // A short interval keeps the first sample before the second.
        assert_eq!(sample_seek_seconds(1.0, 3), vec![0.5, 1.0, 2.0]);
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
        let request = request(10, 170, 96);
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
    fn sprite_grid_is_near_square() {
        assert_eq!(sprite_grid(1), (1, 1));
        assert_eq!(sprite_grid(5), (3, 2));
        assert_eq!(sprite_grid(200), (15, 14));
    }

    #[test]
    fn sprite_places_each_sample_in_its_cell() {
        let request = request(3, 2, 1);
        // Two 2×1 frames: frame 0 is all 10s, frame 1 all 20s; sample 1 has no frame.
        let decoded = DecodedFrames {
            pixels: [vec![10; 6], vec![20; 6]].concat(),
            samples: vec![0, 2],
        };
        let (sprite, columns, rows) = compose_sprite(&decoded, &request);

        assert_eq!((columns, rows), (2, 2));
        assert_eq!((sprite.width(), sprite.height()), (4, 2));
        assert_eq!(sprite.get_pixel(0, 0).0, [10, 10, 10]);
        // Sample 1 (top right) repeats sample 0, the earlier of its equally near neighbours.
        assert_eq!(sprite.get_pixel(2, 0).0, [10, 10, 10]);
        assert_eq!(sprite.get_pixel(1, 1).0, [20, 20, 20]);
        // The cell after the last sample stays empty.
        assert_eq!(sprite.get_pixel(3, 1).0, [0, 0, 0]);
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
    fn decodes_a_video_into_a_sprite_sheet_and_falls_back_to_software() {
        let ffmpeg = repo_ffmpeg();
        let dir =
            std::env::temp_dir().join(format!("ovrley-filmstrip-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("it's a clip.mp4");
        write_test_video(&ffmpeg, &video);
        let request = FilmstripRequest::new(video, 2.0, 3, 32, 18).unwrap();
        // A decoder this platform lacks fails, and the software run takes over.
        let unavailable = if cfg!(target_os = "macos") {
            FilmstripHwaccel::D3d11va
        } else {
            FilmstripHwaccel::Videotoolbox
        };

        let filmstrip = generate_filmstrip(&ffmpeg, &request, Some(unavailable))
            .unwrap()
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!((filmstrip.columns, filmstrip.rows), (2, 2));
        let jpeg = BASE64_STANDARD
            .decode(filmstrip.sprite_jpeg_base64)
            .unwrap();
        let sprite = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((sprite.width(), sprite.height()), (64, 36));
    }

    #[test]
    fn a_file_without_video_yields_no_filmstrip() {
        let ffmpeg = repo_ffmpeg();
        let path = std::env::temp_dir().join(format!(
            "ovrley-filmstrip-empty-{}.mp4",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, b"not a video").unwrap();
        let request = FilmstripRequest::new(path.clone(), 2.0, 3, 32, 18).unwrap();

        let filmstrip = generate_filmstrip(&ffmpeg, &request, None).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(filmstrip, None);
    }
}
