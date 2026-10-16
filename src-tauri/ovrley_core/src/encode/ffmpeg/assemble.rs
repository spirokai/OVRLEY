//! FFmpeg argument builders that join the rendered parts of one output.
//!
//! Composite parts are video-only MP4s. Each is remuxed to MPEG-TS with the
//! Annex-B bitstream filter (parameter sets travel in-band), the parts are joined
//! by the concat demuxer with stream copy, and the audio is encoded once from the
//! original sources. Transparent parts use intra-only MOV codecs and are joined
//! as MOV directly. [`crate::encode::pipeline::assemble`] runs these commands.

use std::path::{Path, PathBuf};

use super::composite::derive_composite_input_window;
use super::composite_filters::format_seconds_arg;

/// Every audio piece is resampled to this rate and stereo before joining.
const JOINED_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// The rendered parts of one output and how to join them.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputAssembly {
    pub target: PathBuf,
    /// Directory holding the parts, their remuxes and the concat manifest.
    pub parts_directory: PathBuf,
    /// Rendered parts in output order, inside `parts_directory`.
    pub parts: Vec<PathBuf>,
    /// Video frames the joined output must contain.
    pub frames: u64,
    pub kind: AssemblyKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssemblyKind {
    Composite {
        hevc: bool,
        /// Display matrix to restore after the MPEG-TS remux (encoders that keep
        /// the source's coded orientation); None when the parts are upright.
        display_rotation: Option<i32>,
        /// One entry per part; None when no source has audio (no audio track).
        audio: Option<Vec<AudioPiece>>,
    },
    Transparent,
}

/// The audio of one part: cut from its source and fitted to the part's video duration.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioPiece {
    /// None for a source without audio: the piece is silent.
    pub source: Option<PathBuf>,
    pub trim_start: f64,
    /// Video duration of the part (its frames at the output FPS).
    pub duration: f64,
}

impl OutputAssembly {
    /// The files the concat demuxer reads: MPEG-TS remuxes for composite parts.
    pub fn joined_parts(&self) -> Vec<PathBuf> {
        match self.kind {
            AssemblyKind::Composite { .. } => self
                .parts
                .iter()
                .map(|part| part.with_extension("ts"))
                .collect(),
            AssemblyKind::Transparent => self.parts.clone(),
        }
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.parts_directory.join("parts.txt")
    }
}

/// Remuxes one composite part to MPEG-TS/Annex-B by stream copy.
pub fn remux_part_args(part: &Path, remux: &Path, hevc: bool) -> Vec<String> {
    let filter = if hevc {
        "hevc_mp4toannexb"
    } else {
        "h264_mp4toannexb"
    };
    [
        "-loglevel",
        "error",
        "-nostdin",
        "-i",
        &part.to_string_lossy(),
        "-map",
        "0:v",
        "-c",
        "copy",
        "-bsf:v",
        filter,
        "-f",
        "mpegts",
        "-y",
        &remux.to_string_lossy(),
    ]
    .map(String::from)
    .to_vec()
}

/// Concat demuxer manifest (UTF-8, no BOM) listing files by name relative to the
/// manifest's own directory, in order.
pub fn concat_manifest(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|file| {
            let name = file
                .file_name()
                .expect("joined parts have file names")
                .to_string_lossy();
            format!("file {}\n", quote_concat_path(&name))
        })
        .collect()
}

/// Quotes a path for a concat manifest: single quotes, with each embedded quote
/// written as `'\''` (close, escaped quote, reopen).
pub fn quote_concat_path(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// Joins the parts listed in the manifest into the target. Composite output
/// stream-copies the video and encodes the audio once from the sources.
pub fn join_args(assembly: &OutputAssembly) -> Vec<String> {
    let mut args: Vec<String> = ["-loglevel", "error", "-nostdin"]
        .map(String::from)
        .to_vec();
    match &assembly.kind {
        AssemblyKind::Composite {
            hevc,
            display_rotation,
            audio,
        } => {
            if let Some(rotation) = display_rotation {
                args.extend(["-display_rotation".into(), rotation.to_string()]);
            }
            push_concat_input(&mut args, assembly);
            if let Some(audio) = audio {
                for piece in audio {
                    if let Some(source) = &piece.source {
                        let window = derive_composite_input_window(piece.trim_start);
                        if window.seek_start > 0.0 {
                            args.extend(["-ss".into(), format_seconds_arg(window.seek_start)]);
                        }
                        args.extend(["-i".into(), source.to_string_lossy().into_owned()]);
                    }
                }
                args.extend(["-filter_complex".into(), audio_graph(audio)]);
            }
            args.extend(["-map".into(), "0:v".into()]);
            if audio.is_some() {
                args.extend(["-map".into(), "[aout]".into()]);
            }
            args.extend(["-c:v".into(), "copy".into()]);
            if *hevc {
                args.extend(["-tag:v".into(), "hvc1".into()]);
            }
            if audio.is_some() {
                args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
            }
            args.extend(["-movflags".into(), "faststart".into()]);
        }
        AssemblyKind::Transparent => {
            push_concat_input(&mut args, assembly);
            args.extend(["-map", "0:v", "-c", "copy"].map(String::from));
        }
    }
    args.extend(["-y".into(), assembly.target.to_string_lossy().into_owned()]);
    args
}

fn push_concat_input(args: &mut Vec<String>, assembly: &OutputAssembly) {
    args.extend([
        "-f".into(),
        "concat".into(),
        "-i".into(),
        assembly.manifest_path().to_string_lossy().into_owned(),
    ]);
}

/// One audio stream per part, each exactly as long as the part's video, then
/// concatenated. Parts with a source read inputs 1.. in order.
pub fn audio_graph(audio: &[AudioPiece]) -> String {
    let mut input = 0;
    let mut chains = Vec::with_capacity(audio.len() + 1);
    for (index, piece) in audio.iter().enumerate() {
        let duration = format_seconds_arg(piece.duration);
        chains.push(match &piece.source {
            Some(_) => {
                input += 1;
                let window = derive_composite_input_window(piece.trim_start);
                format!(
                    "[{input}:a]atrim=start={}:duration={duration},asetpts=N/SR/TB,\
                     aresample={JOINED_AUDIO_SAMPLE_RATE},\
                     aformat=sample_fmts=fltp:channel_layouts=stereo,\
                     apad=whole_dur={duration}[a{index}]",
                    format_seconds_arg(window.filter_start),
                )
            }
            None => format!(
                "anullsrc=r={JOINED_AUDIO_SAMPLE_RATE}:cl=stereo,atrim=duration={duration}[a{index}]"
            ),
        });
    }
    let labels: String = (0..audio.len())
        .map(|index| format!("[a{index}]"))
        .collect();
    chains.push(format!("{labels}concat=n={}:v=0:a=1[aout]", audio.len()));
    chains.join(";")
}

/// Counts the video packets and lists the streams of a joined output.
pub fn probe_output_args(target: &Path) -> Vec<String> {
    [
        "-v",
        "error",
        "-count_packets",
        "-show_entries",
        "stream=codec_type,nb_read_packets",
        "-of",
        "json",
        &target.to_string_lossy(),
    ]
    .map(String::from)
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composite(
        audio: Option<Vec<AudioPiece>>,
        hevc: bool,
        rotation: Option<i32>,
    ) -> OutputAssembly {
        OutputAssembly {
            target: "/out/joined.mp4".into(),
            parts_directory: "/out/.parts".into(),
            parts: vec![
                "/out/.parts/part-0000.mp4".into(),
                "/out/.parts/part-0001.mp4".into(),
            ],
            frames: 90,
            kind: AssemblyKind::Composite {
                hevc,
                display_rotation: rotation,
                audio,
            },
        }
    }

    #[test]
    fn manifests_quote_every_name_and_escape_embedded_quotes() {
        assert_eq!(
            concat_manifest(&["/x/part-0000.ts".into(), "/x/it's here.ts".into()]),
            "file 'part-0000.ts'\nfile 'it'\\''s here.ts'\n"
        );
    }

    #[test]
    fn composite_parts_remux_to_annex_b_transport_streams() {
        let args = remux_part_args(Path::new("/p/part.mp4"), Path::new("/p/part.ts"), true);
        assert_eq!(
            args.join(" "),
            "-loglevel error -nostdin -i /p/part.mp4 -map 0:v -c copy -bsf:v hevc_mp4toannexb -f mpegts -y /p/part.ts"
        );
        assert!(
            remux_part_args(Path::new("a.mp4"), Path::new("a.ts"), false)
                .contains(&"h264_mp4toannexb".to_string())
        );
        assert_eq!(
            composite(None, false, None).joined_parts(),
            vec![
                PathBuf::from("/out/.parts/part-0000.ts"),
                PathBuf::from("/out/.parts/part-0001.ts")
            ]
        );
    }

    #[test]
    fn the_audio_pass_cuts_every_source_once_and_fills_silent_parts() {
        let audio = vec![
            AudioPiece {
                source: Some("/v/a.mp4".into()),
                trim_start: 12.5,
                duration: 1.001,
            },
            AudioPiece {
                source: None,
                trim_start: 0.0,
                duration: 2.0,
            },
            AudioPiece {
                source: Some("/v/c.mp4".into()),
                trim_start: 1.0,
                duration: 0.5,
            },
        ];
        let args = join_args(&composite(Some(audio), true, None));
        assert_eq!(
            args.join(" "),
            "-loglevel error -nostdin -f concat -i /out/.parts/parts.txt \
             -ss 7.5 -i /v/a.mp4 -i /v/c.mp4 -filter_complex \
             [1:a]atrim=start=5:duration=1.001,asetpts=N/SR/TB,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur=1.001[a0];\
             anullsrc=r=48000:cl=stereo,atrim=duration=2[a1];\
             [2:a]atrim=start=1:duration=0.5,asetpts=N/SR/TB,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur=0.5[a2];\
             [a0][a1][a2]concat=n=3:v=0:a=1[aout] \
             -map 0:v -map [aout] -c:v copy -tag:v hvc1 -c:a aac -b:a 192k -movflags faststart -y /out/joined.mp4"
        );
    }

    #[test]
    fn outputs_without_audio_copy_video_only_and_restore_rotation() {
        assert_eq!(
            join_args(&composite(None, false, Some(270))).join(" "),
            "-loglevel error -nostdin -display_rotation 270 -f concat -i /out/.parts/parts.txt \
             -map 0:v -c:v copy -movflags faststart -y /out/joined.mp4"
        );
    }

    #[test]
    fn transparent_parts_join_as_mov() {
        let assembly = OutputAssembly {
            target: "/out/joined.mov".into(),
            parts_directory: "/out/.parts".into(),
            parts: vec!["/out/.parts/part-0000.mov".into()],
            frames: 10,
            kind: AssemblyKind::Transparent,
        };
        assert_eq!(assembly.joined_parts(), assembly.parts);
        assert_eq!(
            join_args(&assembly).join(" "),
            "-loglevel error -nostdin -f concat -i /out/.parts/parts.txt -map 0:v -c copy -y /out/joined.mov"
        );
    }
}
