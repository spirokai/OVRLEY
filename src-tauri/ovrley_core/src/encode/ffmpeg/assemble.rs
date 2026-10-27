//! FFmpeg argument builders that join the rendered segments of one output.
//!
//! Composite segments are video-only MP4s. At every transition the outgoing
//! piece's tail and the incoming piece's head are blended with `xfade` and
//! encoded with the output's own encoder and settings. Every segment is then
//! remuxed to MPEG-TS with the Annex-B bitstream filter (parameter sets travel
//! in-band), the segments are joined by the concat demuxer with stream copy, and
//! the audio is encoded once from the original sources (`acrossfade` at
//! transitions). Transparent segments use intra-only MOV codecs, join with hard
//! cuts only and are concatenated as MOV directly.
//! [`crate::encode::pipeline::assemble`] runs these commands.

use std::path::{Path, PathBuf};

use super::catalog::{CompositeCodecId, CompositeFilterStackKind};
use super::composite::derive_composite_input_window;
use super::composite_filters::format_seconds_arg;
use super::composite_profiles::composite_profile;
use crate::encode::fps::Fps;
use crate::encode::quality::{rate_control_args, EncodingQuality};

/// Every audio piece is resampled to this rate and stereo before joining.
const JOINED_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// The rendered segments of one output and how to join them.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputAssembly {
    pub target: PathBuf,
    /// Directory holding the segments, transitions, remuxes and the concat manifest.
    pub parts_directory: PathBuf,
    /// The output in order.
    pub segments: Vec<AssemblySegment>,
    /// Video frames the joined output must contain.
    pub frames: u64,
    pub kind: AssemblyKind,
}

/// One stretch of the joined output.
#[derive(Clone, Debug, PartialEq)]
pub enum AssemblySegment {
    /// A rendered segment shown as is.
    Part(PathBuf),
    /// An `xfade` of the outgoing tail into the incoming head (both exactly
    /// `duration` long), encoded to `output`.
    Transition {
        outgoing: PathBuf,
        incoming: PathBuf,
        name: String,
        duration: f64,
        output: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssemblyKind {
    Composite {
        hevc: bool,
        /// Display matrix to restore after the MPEG-TS remux (encoders that keep
        /// the source's coded orientation); None when the parts are upright.
        display_rotation: Option<i32>,
        transition_encoding: TransitionEncoding,
        /// None when no source has audio (no audio track).
        audio: Option<AssemblyAudio>,
    },
    Transparent,
}

/// The audio of a joined output: one entry per piece, and per join between two
/// pieces its crossfade in seconds (0 for a hard cut).
#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyAudio {
    pub pieces: Vec<AudioPiece>,
    pub crossfades: Vec<f64>,
}

/// The audio of one piece: cut from its source and fitted to the piece's video duration.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioPiece {
    /// None for a source without audio: the piece is silent.
    pub source: Option<PathBuf>,
    pub trim_start: f64,
    /// Video duration of the piece (its frames at the output FPS).
    pub duration: f64,
}

/// How transition segments are encoded: the output's encoder, profile output
/// arguments and rate control, fed with decoded system-memory frames. Encoders
/// that only take hardware frames get their device and an upload filter.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionEncoding {
    pub device_args: Vec<String>,
    /// Appended to the `xfade` filter: the parts' pixel format, plus the upload
    /// for hardware-frame encoders. `xfade` otherwise negotiates 4:4:4 with
    /// encoders that accept it, and a 4:4:4 stretch stream-copied between 4:2:0
    /// parts does not decode in players that keep the first parameter sets.
    pub output_filter: String,
    pub encoder_args: Vec<String>,
}

impl TransitionEncoding {
    pub fn for_codec(codec_id: CompositeCodecId, quality: EncodingQuality, fps: Fps) -> Self {
        let profile = composite_profile(codec_id);
        // Parts are 4:2:0: 8-bit unless the profile pins its own pixel format.
        let part_format = profile
            .output_args
            .iter()
            .position(|arg| *arg == "-pix_fmt")
            .map_or("yuv420p", |index| profile.output_args[index + 1]);
        let (device_args, output_filter): (Vec<&str>, _) =
            match codec_id.metadata().filter_stack_kind {
                CompositeFilterStackKind::VaapiOverlay => (
                    vec!["-init_hw_device", "vaapi=va", "-filter_hw_device", "va"],
                    "format=nv12,hwupload".to_string(),
                ),
                CompositeFilterStackKind::AmfD3d11Overlay => (
                    profile.input_args.to_vec(),
                    "format=nv12,hwupload".to_string(),
                ),
                CompositeFilterStackKind::SoftwareOverlay
                | CompositeFilterStackKind::CudaOverlay
                | CompositeFilterStackKind::QsvFullOverlay => {
                    (Vec::new(), format!("format={part_format}"))
                }
            };
        let mut encoder_args = vec![
            "-r".to_string(),
            fps.ffmpeg_arg(),
            "-c:v".to_string(),
            profile
                .codec_id
                .metadata()
                .encoder_id
                .metadata()
                .ffmpeg_name
                .to_string(),
        ];
        encoder_args.extend(profile.output_args.iter().map(|arg| (*arg).to_string()));
        encoder_args.extend(rate_control_args(profile, quality));
        Self {
            device_args: device_args.into_iter().map(String::from).collect(),
            output_filter,
            encoder_args,
        }
    }
}

impl OutputAssembly {
    /// The files the concat demuxer reads, in order: MPEG-TS remuxes of every
    /// composite segment (transitions included), the parts themselves otherwise.
    pub fn joined_parts(&self) -> Vec<PathBuf> {
        self.segments
            .iter()
            .map(|segment| {
                let file = match segment {
                    AssemblySegment::Part(part) => part,
                    AssemblySegment::Transition { output, .. } => output,
                };
                match self.kind {
                    AssemblyKind::Composite { .. } => file.with_extension("ts"),
                    AssemblyKind::Transparent => file.clone(),
                }
            })
            .collect()
    }

    /// The encoded files to remux before joining, paired with their remuxes.
    pub fn remuxes(&self) -> Vec<(PathBuf, PathBuf)> {
        self.segments
            .iter()
            .map(|segment| match segment {
                AssemblySegment::Part(part) => part.clone(),
                AssemblySegment::Transition { output, .. } => output.clone(),
            })
            .zip(self.joined_parts())
            .collect()
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.parts_directory.join("parts.txt")
    }
}

/// Blends the outgoing tail into the incoming head and encodes the result with
/// the output's encoder. Inputs keep their coded orientation (`-noautorotate`).
pub fn transition_segment_args(
    encoding: &TransitionEncoding,
    segment: &AssemblySegment,
) -> Vec<String> {
    let AssemblySegment::Transition {
        outgoing,
        incoming,
        name,
        duration,
        output,
    } = segment
    else {
        panic!("only transition segments are encoded");
    };
    let mut args: Vec<String> = ["-loglevel", "error", "-nostdin"]
        .map(String::from)
        .to_vec();
    args.extend(encoding.device_args.iter().cloned());
    for input in [outgoing, incoming] {
        args.extend([
            "-noautorotate".into(),
            "-i".into(),
            input.to_string_lossy().into_owned(),
        ]);
    }
    args.extend([
        "-filter_complex".into(),
        format!(
            "[0:v][1:v]xfade=transition={name}:duration={}:offset=0,{}[out]",
            format_seconds_arg(*duration),
            encoding.output_filter
        ),
        "-map".into(),
        "[out]".into(),
    ]);
    args.extend(encoding.encoder_args.iter().cloned());
    args.extend(["-an", "-movflags", "faststart", "-y"].map(String::from));
    args.push(output.to_string_lossy().into_owned());
    args
}

/// Remuxes one composite segment to MPEG-TS/Annex-B by stream copy.
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

/// Joins the segments listed in the manifest into the target. Composite output
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
            ..
        } => {
            if let Some(rotation) = display_rotation {
                args.extend(["-display_rotation".into(), rotation.to_string()]);
            }
            push_concat_input(&mut args, assembly);
            if let Some(audio) = audio {
                for piece in &audio.pieces {
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

/// One audio stream per piece, each exactly as long as the piece's video, joined
/// in order: `acrossfade` over a transition's duration, `concat` at a hard cut.
/// Pieces with a source read inputs 1.. in order.
pub fn audio_graph(audio: &AssemblyAudio) -> String {
    let mut input = 0;
    let mut chains = Vec::with_capacity(2 * audio.pieces.len());
    for (index, piece) in audio.pieces.iter().enumerate() {
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
    let last = audio.crossfades.len();
    let mut joined = "a0".to_string();
    for (index, crossfade) in audio.crossfades.iter().enumerate() {
        let next = if index + 1 == last {
            "aout".to_string()
        } else {
            format!("j{}", index + 1)
        };
        chains.push(if *crossfade > 0.0 {
            format!(
                "[{joined}][a{}]acrossfade=d={}:c1=tri:c2=tri[{next}]",
                index + 1,
                format_seconds_arg(*crossfade)
            )
        } else {
            format!("[{joined}][a{}]concat=n=2:v=0:a=1[{next}]", index + 1)
        });
        joined = next;
    }
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
    use crate::encode::quality::EncodingQuality;

    fn software(hevc: bool) -> TransitionEncoding {
        TransitionEncoding::for_codec(
            if hevc {
                CompositeCodecId::SoftwareHevc
            } else {
                CompositeCodecId::SoftwareH264
            },
            EncodingQuality::Quality(20),
            Fps::new(30000, 1001).unwrap(),
        )
    }

    fn transition() -> AssemblySegment {
        AssemblySegment::Transition {
            outgoing: "/out/.parts/part-0001.mp4".into(),
            incoming: "/out/.parts/part-0002.mp4".into(),
            name: "wipeleft".into(),
            duration: 1.001,
            output: "/out/.parts/transition-0000.mp4".into(),
        }
    }

    fn composite(
        audio: Option<AssemblyAudio>,
        hevc: bool,
        rotation: Option<i32>,
    ) -> OutputAssembly {
        OutputAssembly {
            target: "/out/joined.mp4".into(),
            parts_directory: "/out/.parts".into(),
            segments: vec![
                AssemblySegment::Part("/out/.parts/part-0000.mp4".into()),
                transition(),
                AssemblySegment::Part("/out/.parts/part-0003.mp4".into()),
            ],
            frames: 90,
            kind: AssemblyKind::Composite {
                hevc,
                display_rotation: rotation,
                transition_encoding: software(hevc),
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
    fn composite_segments_remux_to_annex_b_transport_streams() {
        let args = remux_part_args(Path::new("/p/part.mp4"), Path::new("/p/part.ts"), true);
        assert_eq!(
            args.join(" "),
            "-loglevel error -nostdin -i /p/part.mp4 -map 0:v -c copy -bsf:v hevc_mp4toannexb -f mpegts -y /p/part.ts"
        );
        assert!(
            remux_part_args(Path::new("a.mp4"), Path::new("a.ts"), false)
                .contains(&"h264_mp4toannexb".to_string())
        );
        let assembly = composite(None, false, None);
        assert_eq!(
            assembly.joined_parts(),
            vec![
                PathBuf::from("/out/.parts/part-0000.ts"),
                PathBuf::from("/out/.parts/transition-0000.ts"),
                PathBuf::from("/out/.parts/part-0003.ts")
            ]
        );
        assert_eq!(
            assembly.remuxes()[1],
            (
                PathBuf::from("/out/.parts/transition-0000.mp4"),
                PathBuf::from("/out/.parts/transition-0000.ts")
            )
        );
    }

    #[test]
    fn transitions_blend_tail_into_head_with_the_output_encoder() {
        assert_eq!(
            transition_segment_args(&software(false), &transition()).join(" "),
            "-loglevel error -nostdin -noautorotate -i /out/.parts/part-0001.mp4 -noautorotate -i /out/.parts/part-0002.mp4 \
             -filter_complex [0:v][1:v]xfade=transition=wipeleft:duration=1.001:offset=0,format=yuv420p[out] -map [out] \
             -r 30000/1001 -c:v libx264 -preset veryfast -crf 20 -an -movflags faststart -y /out/.parts/transition-0000.mp4"
        );
        let hevc = software(true);
        assert_eq!(
            hevc.encoder_args.join(" "),
            "-r 30000/1001 -c:v libx265 -pix_fmt yuv420p10le -profile:v main10 -preset veryfast -tag:v hvc1 -crf 20"
        );
        // The blend keeps the parts' pixel format, the profile's own when it pins one.
        assert_eq!(hevc.output_filter, "format=yuv420p10le");
        // Hardware-frame encoders get their device and an upload; the others take system frames.
        let fps = Fps::new(30, 1).unwrap();
        let vaapi = TransitionEncoding::for_codec(
            CompositeCodecId::VaapiH264,
            EncodingQuality::Quality(20),
            fps,
        );
        assert_eq!(
            vaapi.device_args,
            ["-init_hw_device", "vaapi=va", "-filter_hw_device", "va"]
        );
        assert_eq!(vaapi.output_filter, "format=nv12,hwupload");
        let amf = TransitionEncoding::for_codec(
            CompositeCodecId::AmfH264,
            EncodingQuality::Quality(20),
            fps,
        );
        assert_eq!(
            amf.device_args,
            ["-init_hw_device", "d3d11va=dx", "-filter_hw_device", "dx"]
        );
        for codec in [
            CompositeCodecId::NnvgpuHevc,
            CompositeCodecId::QsvFullH264,
            CompositeCodecId::MacH264,
        ] {
            let encoding = TransitionEncoding::for_codec(codec, EncodingQuality::Quality(20), fps);
            assert!(
                encoding.device_args.is_empty() && encoding.output_filter == "format=yuv420p",
                "{codec:?}"
            );
        }
    }

    #[test]
    fn the_audio_pass_cuts_every_source_once_and_crossfades_at_transitions() {
        let audio = AssemblyAudio {
            pieces: vec![
                AudioPiece {
                    source: Some("/v/a.mp4".into()),
                    trim_start: 12.5,
                    duration: 3.0,
                },
                AudioPiece {
                    source: None,
                    trim_start: 0.0,
                    duration: 2.0,
                },
                AudioPiece {
                    source: Some("/v/c.mp4".into()),
                    trim_start: 1.0,
                    duration: 1.5,
                },
            ],
            crossfades: vec![1.001, 0.0],
        };
        let args = join_args(&composite(Some(audio), true, None));
        assert_eq!(
            args.join(" ").replace('\\', "/"),
            "-loglevel error -nostdin -f concat -i /out/.parts/parts.txt \
             -ss 7.5 -i /v/a.mp4 -i /v/c.mp4 -filter_complex \
             [1:a]atrim=start=5:duration=3,asetpts=N/SR/TB,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur=3[a0];\
             anullsrc=r=48000:cl=stereo,atrim=duration=2[a1];\
             [2:a]atrim=start=1:duration=1.5,asetpts=N/SR/TB,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur=1.5[a2];\
             [a0][a1]acrossfade=d=1.001:c1=tri:c2=tri[j1];\
             [j1][a2]concat=n=2:v=0:a=1[aout] \
             -map 0:v -map [aout] -c:v copy -tag:v hvc1 -c:a aac -b:a 192k -movflags faststart -y /out/joined.mp4"
        );
    }

    #[test]
    fn outputs_without_audio_copy_video_only_and_restore_rotation() {
        assert_eq!(
            join_args(&composite(None, false, Some(270))).join(" ").replace('\\', "/"),
            "-loglevel error -nostdin -display_rotation 270 -f concat -i /out/.parts/parts.txt \
             -map 0:v -c:v copy -movflags faststart -y /out/joined.mp4"
        );
    }

    #[test]
    fn transparent_parts_join_as_mov() {
        let assembly = OutputAssembly {
            target: "/out/joined.mov".into(),
            parts_directory: "/out/.parts".into(),
            segments: vec![AssemblySegment::Part("/out/.parts/part-0000.mov".into())],
            frames: 10,
            kind: AssemblyKind::Transparent,
        };
        assert_eq!(
            assembly.joined_parts(),
            [PathBuf::from("/out/.parts/part-0000.mov")]
        );
        assert_eq!(
            join_args(&assembly).join(" ").replace('\\', "/"),
            "-loglevel error -nostdin -f concat -i /out/.parts/parts.txt -map 0:v -c copy -y /out/joined.mov"
        );
    }
}
