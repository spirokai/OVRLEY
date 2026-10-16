//! Transition proxies: short preview clips of one join, `xfade(tail(A), head(B))`
//! with `acrossfade` audio, rendered from the sources at preview resolution with a
//! short GOP so the editor preview can seek inside them. Proxies live in the
//! runtime temp directory and are cleared at startup.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use super::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use super::composite_filters::format_seconds_arg;
use super::transitions::is_xfade_transition;
use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;

const PROXY_DIRECTORY: &str = "transition-proxies";
const PROXY_SAMPLE_RATE: u32 = 48_000;
/// A keyframe every few frames keeps seeking inside the proxy immediate.
const PROXY_GOP_FRAMES: u32 = 10;
/// Proxy files of a session; the directory is cleared at startup.
static NEXT_PROXY: AtomicU64 = AtomicU64::new(0);

/// The window of one source a proxy reads.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionProxySource {
    pub path: String,
    /// Source second where the window starts.
    pub start_second: f64,
    pub has_audio: bool,
}

/// One join to preview: the last `durationSeconds` of the outgoing piece blended
/// into the first `durationSeconds` of the incoming piece.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionProxyRequest {
    pub outgoing: TransitionProxySource,
    pub incoming: TransitionProxySource,
    pub transition: String,
    pub duration_seconds: f64,
    /// Even proxy dimensions; both sources are fitted into them.
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl TransitionProxyRequest {
    fn validate(&self) -> CoreResult<Fps> {
        if !is_xfade_transition(&self.transition) {
            return Err(CoreError::Config(format!(
                "Unknown transition {}",
                self.transition
            )));
        }
        if !self.duration_seconds.is_finite() || self.duration_seconds <= 0.0 {
            return Err(CoreError::Config(
                "Transition proxy duration must be positive".into(),
            ));
        }
        for source in [&self.outgoing, &self.incoming] {
            if source.path.trim().is_empty()
                || !source.start_second.is_finite()
                || source.start_second < 0.0
            {
                return Err(CoreError::Config(
                    "Transition proxy sources need a path and a start second of 0 or more".into(),
                ));
            }
        }
        if self.width == 0
            || self.height == 0
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            return Err(CoreError::Config(
                "Transition proxy dimensions must be even and positive".into(),
            ));
        }
        Fps::new(self.fps_num, self.fps_den)
    }
}

/// FFmpeg arguments that render one proxy into `output`.
pub fn transition_proxy_args(request: &TransitionProxyRequest, output: &Path) -> Vec<String> {
    let duration = format_seconds_arg(request.duration_seconds);
    let (width, height) = (request.width, request.height);
    let video = |input: usize| {
        format!(
            "[{input}:v]fps={}/{},scale={width}:{height}:force_original_aspect_ratio=decrease,\
             pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p,setpts=PTS-STARTPTS[v{input}]",
            request.fps_num, request.fps_den
        )
    };
    let audio = |input: usize, source: &TransitionProxySource| {
        if source.has_audio {
            format!(
                "[{input}:a]aresample={PROXY_SAMPLE_RATE},aformat=sample_fmts=fltp:channel_layouts=stereo,\
                 apad=whole_dur={duration},atrim=duration={duration},asetpts=N/SR/TB[a{input}]"
            )
        } else {
            format!("anullsrc=r={PROXY_SAMPLE_RATE}:cl=stereo,atrim=duration={duration}[a{input}]")
        }
    };
    let graph = [
        video(0),
        video(1),
        format!(
            "[v0][v1]xfade=transition={}:duration={duration}:offset=0[v]",
            request.transition
        ),
        audio(0, &request.outgoing),
        audio(1, &request.incoming),
        format!("[a0][a1]acrossfade=d={duration}:c1=tri:c2=tri[a]"),
    ]
    .join(";");
    let mut args: Vec<String> = ["-loglevel", "error", "-nostdin"]
        .map(String::from)
        .to_vec();
    for source in [&request.outgoing, &request.incoming] {
        args.extend([
            "-ss".into(),
            format_seconds_arg(source.start_second),
            "-t".into(),
            duration.clone(),
            "-i".into(),
            source.path.clone(),
        ]);
    }
    args.extend(["-filter_complex".into(), graph]);
    args.extend(
        [
            "-map",
            "[v]",
            "-map",
            "[a]",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "26",
            "-g",
        ]
        .map(String::from),
    );
    args.push(PROXY_GOP_FRAMES.to_string());
    args.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "faststart",
            "-y",
        ]
        .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    args
}

fn proxy_directory(paths: &AppPaths) -> PathBuf {
    paths.temp_dir.join(PROXY_DIRECTORY)
}

/// Renders one proxy to a new file in the proxy directory and returns its path.
pub fn render_transition_proxy(
    paths: &AppPaths,
    request: &TransitionProxyRequest,
) -> CoreResult<PathBuf> {
    request.validate()?;
    let directory = proxy_directory(paths);
    std::fs::create_dir_all(&directory).map_err(|source| CoreError::Io {
        path: directory.clone(),
        source,
    })?;
    let output = directory.join(format!(
        "proxy-{}-{}.mp4",
        std::process::id(),
        NEXT_PROXY.fetch_add(1, Ordering::Relaxed)
    ));
    let mut command = Command::new(resolve_ffmpeg_binary(&paths.repo_root)?);
    configure_ffmpeg_command(&mut command);
    let result = command
        .args(transition_proxy_args(request, &output))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| CoreError::Encode(format!("Could not start ffmpeg: {error}")))?;
    if !result.status.success() {
        if output.exists() {
            remove_transition_proxy(&output)?;
        }
        return Err(CoreError::Encode(format!(
            "ffmpeg could not render the transition preview: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(output)
}

/// Deletes one rendered proxy.
pub fn remove_transition_proxy(path: &Path) -> CoreResult<()> {
    std::fs::remove_file(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Removes the proxies of earlier sessions.
pub fn clear_transition_proxies(paths: &AppPaths) -> CoreResult<()> {
    let directory = proxy_directory(paths);
    match std::fs::remove_dir_all(&directory) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(CoreError::Io {
            path: directory,
            source,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn request(has_audio: bool) -> TransitionProxyRequest {
        TransitionProxyRequest {
            outgoing: TransitionProxySource {
                path: "/v/a.mp4".into(),
                start_second: 8.5,
                has_audio,
            },
            incoming: TransitionProxySource {
                path: "/v/b.mp4".into(),
                start_second: 0.0,
                has_audio: false,
            },
            transition: "wipeleft".into(),
            duration_seconds: 1.5,
            width: 640,
            height: 360,
            fps_num: 30000,
            fps_den: 1001,
        }
    }

    #[test]
    fn proxies_blend_both_windows_with_crossfaded_audio() {
        assert_eq!(
            transition_proxy_args(&request(true), Path::new("/tmp/p.mp4")).join(" "),
            "-loglevel error -nostdin -ss 8.5 -t 1.5 -i /v/a.mp4 -ss 0 -t 1.5 -i /v/b.mp4 -filter_complex \
             [0:v]fps=30000/1001,scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p,setpts=PTS-STARTPTS[v0];\
             [1:v]fps=30000/1001,scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p,setpts=PTS-STARTPTS[v1];\
             [v0][v1]xfade=transition=wipeleft:duration=1.5:offset=0[v];\
             [0:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur=1.5,atrim=duration=1.5,asetpts=N/SR/TB[a0];\
             anullsrc=r=48000:cl=stereo,atrim=duration=1.5[a1];\
             [a0][a1]acrossfade=d=1.5:c1=tri:c2=tri[a] \
             -map [v] -map [a] -c:v libx264 -preset ultrafast -crf 26 -g 10 -c:a aac -b:a 128k -movflags faststart -y /tmp/p.mp4"
        );
    }

    #[test]
    fn requests_are_validated() {
        let mut unknown = request(true);
        unknown.transition = "spin".into();
        let mut odd = request(true);
        odd.width = 641;
        let mut negative = request(true);
        negative.incoming.start_second = -1.0;
        let mut empty = request(true);
        empty.duration_seconds = 0.0;
        for invalid in [unknown, odd, negative, empty] {
            assert!(invalid.validate().is_err(), "{invalid:?}");
        }
        assert!(request(false).validate().is_ok());
    }
}
