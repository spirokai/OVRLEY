//! FFmpeg argument builder for MP4 compositing mode.
//!
//! Consumes fixed job timing and source metadata to construct codec arguments.
//! These are immutable settings; [`crate::encode::pipeline`] owns execution,
//! supervision and cleanup for both export modes.

use crate::output::RenderOutputTarget;
use std::path::{Path, PathBuf};

use crate::encode::ffmpeg::catalog::{CompositeCodecId, CompositeFilterStackKind};
use crate::encode::overlay_format::{OverlayPixelFormat, YuvColor};
use crate::encode::plan::CompositeRenderPlan;
use crate::encode::quality::rate_control_args;
use crate::error::{CoreError, CoreResult};
use crate::render::{AtlasPlan, FrameSize};

use super::composite_filters::{
    composite_filter_complex, cuda_display_metadata_filter, format_seconds_arg,
    normalize_source_rotation, qsv_overlay_cpu_rotation_filter, source_rotation_filter,
};
use super::composite_profiles::composite_profile;

/// Prepared composite settings and diagnostic metadata for one encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeEncoding {
    pub render: CompositeRenderPlan,
    pub frame_size: FrameSize,
    pub ffmpeg_settings: CompositeFfmpegSettings,
    pub output_path: PathBuf,
}

impl CompositeEncoding {
    pub fn new(
        frame_size: FrameSize,
        render: CompositeRenderPlan,
        atlas: &AtlasPlan,
        include_audio: bool,
        audio_bit_rate: Option<u64>,
        source_rotation_degrees: Option<i32>,
        source_color: Option<YuvColor>,
        output_target: &RenderOutputTarget,
    ) -> CoreResult<Self> {
        let ffmpeg_settings = build_composite_ffmpeg_settings(
            &render,
            frame_size,
            atlas,
            include_audio,
            audio_bit_rate,
            source_rotation_degrees,
            source_color,
        )?;
        let output_path = output_target.path().to_path_buf();

        Ok(Self {
            render,
            frame_size,
            ffmpeg_settings,
            output_path,
        })
    }
}

/// Grouped FFmpeg arguments needed to spawn a composite render.
///
/// The caller is expected to concatenate these groups in order and append the
/// output path if its process-spawning architecture owns destination handling.
///
/// Composite mode currently uses three inputs:
/// - input 0: coarsely seeked source video for frame-accurate filter-side video trim
/// - input 1: raw atlas frames from stdin (`pipe:0`) in `overlay_pixel_format`
/// - input 2: coarsely seeked source media for filtered audio encoding
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeFfmpegSettings {
    pub codec_id: CompositeCodecId,
    pub overlay_pixel_format: OverlayPixelFormat,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub input_2_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}

/// Keeps a small source-local preroll after the input seek so the filter graph
/// remains responsible for the exact export boundary.
///
/// The input seek is intentionally an implementation detail derived from the
/// already validated `trim_start`; it is not part of the render config.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompositeInputWindow {
    seek_start: f64,
    filter_start: f64,
}

const COMPOSITE_INPUT_SEEK_PREROLL_SECONDS: f64 = 5.0;

fn derive_composite_input_window(trim_start: f64) -> CompositeInputWindow {
    let seek_start = (trim_start - COMPOSITE_INPUT_SEEK_PREROLL_SECONDS).max(0.0);
    CompositeInputWindow {
        seek_start,
        filter_start: trim_start - seek_start,
    }
}

impl CompositeFfmpegSettings {
    /// Returns the canonical FFmpeg argv sequence for this composite encode.
    pub fn command_args(&self, output_path: &Path) -> Vec<String> {
        let mut args = vec!["-loglevel".to_string(), "info".to_string()];
        args.extend(self.input_0_args.iter().cloned());
        args.extend(self.input_1_args.iter().cloned());
        args.extend(self.input_2_args.iter().cloned());
        args.extend(["-filter_complex".to_string(), self.filter_complex.clone()]);
        args.extend(self.output_args.iter().cloned());
        args.push(output_path.to_string_lossy().into_owned());
        args
    }
}

/// Builds FFmpeg argument groups from the canonical composite render plan.
///
/// Input 0 uses a coarse input seek followed by frame-accurate filter-side
/// video trimming. Input 1 is the raw overlay stream on `pipe:0`, and
/// input 2 uses the same derived source window for filtered audio encoding.
/// Audio encoding targets the source bitrate, or 192 kbps when unreported.
///
/// The overlay is piped as frames of `atlas`, which the filter graph crops and
/// overlays slot by slot. The QSV full overlay stack places each slot on the
/// coded main video, turned to its coded orientation.
///
/// The overlay is piped as `yuva420p` in the source's colour (`source_color`)
/// for every graph that blends in YUV. The QSV full-overlay graph uploads RGBA
/// surfaces, and a source colour space without a supported matrix
/// (`source_color` None) keeps RGBA for FFmpeg to convert. An untagged HD
/// source is treated as BT.709 and its main video tagged `bt709`, so the
/// overlay blends, and the output is labelled, as players display it.
///
/// The function consumes the validated render plan, selects and configures an encoder profile,
/// then assembles four argument groups that callers concatenate in order: HW
/// init args, input 0 (seeked and filter-trimmed video), input 1 (overlay pipe),
/// input 2 (seeked and filter-trimmed audio source), filter_complex, and output
/// args.
pub fn build_composite_ffmpeg_settings(
    render: &CompositeRenderPlan,
    frame_size: FrameSize,
    atlas: &AtlasPlan,
    include_audio: bool,
    audio_bit_rate: Option<u64>,
    source_rotation_degrees: Option<i32>,
    source_color: Option<YuvColor>,
) -> CoreResult<CompositeFfmpegSettings> {
    let FrameSize { width, height } = frame_size;
    // Establish one canonical rotation value before profile selection and reuse
    // it for filters, dimensions, autorotation, and output metadata.
    let source_rotation_degrees = normalize_source_rotation(source_rotation_degrees)?;
    // ── PHASE 1: VALIDATE INPUTS & REDUCE FPS ──
    let video_path = render.video_path.to_string_lossy().into_owned();

    // ── PHASE 2: SELECT & CONFIGURE PROFILE ──
    let mut selected_profile = composite_profile(render.requested_codec_id);
    let source_rotation_filter = source_rotation_filter(
        source_rotation_degrees,
        selected_profile.codec_id.metadata().filter_stack_kind,
    );
    if source_rotation_filter.is_some() {
        let has_gpu_rotation = matches!(
            selected_profile.codec_id.metadata().filter_stack_kind,
            CompositeFilterStackKind::CudaOverlay
                | CompositeFilterStackKind::QsvFullOverlay
                | CompositeFilterStackKind::VaapiOverlay
        );
        if !has_gpu_rotation {
            if let Some(fallback_profile) = selected_profile.codec_id.metadata().fallback_profile {
                log::info!(
                    "Composite source rotation requires CPU orientation filters; using {} instead of {}",
                    fallback_profile.metadata().profile_name,
                    selected_profile.codec_id.metadata().profile_name
                );
                selected_profile = composite_profile(fallback_profile);
            }
        }
    }
    if matches!(
        selected_profile.codec_id.metadata().filter_stack_kind,
        CompositeFilterStackKind::QsvFullOverlay
    )
        && render.qsv_full_init_args.is_empty()
    {
        return Err(CoreError::Encode(format!(
            "Requested experimental QSV overlay profile {} is unavailable; codec detection did not provide working QSV hardware-device init args.",
            selected_profile.codec_id.metadata().profile_name
        )));
    }

    let filter_stack_kind = selected_profile.codec_id.metadata().filter_stack_kind;
    let qsv_full_overlay = matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay);
    let qsv_overlay_cpu_rotation_filter =
        qsv_overlay_cpu_rotation_filter(source_rotation_degrees, filter_stack_kind);
    let input_window = derive_composite_input_window(render.trim_start);

    // ── PHASE 3: BUILD INPUT 0 ARGS (coarse seek plus filter-side trim) ──
    let mut input_0_args = if matches!(
        selected_profile.codec_id.metadata().filter_stack_kind,
        CompositeFilterStackKind::QsvFullOverlay
    ) {
        render.qsv_full_init_args.clone()
    } else {
        selected_profile
            .input_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect()
    };
    if input_window.seek_start > 0.0 {
        input_0_args.extend([
            "-ss".to_string(),
            format_seconds_arg(input_window.seek_start),
        ]);
    }
    // QSV-full must retain coded main-video surfaces for zero-copy compositing.
    // Other profiles keep their existing behavior: disable FFmpeg autorotation
    // only when the selected graph physically applies a rotation filter.
    let source_autorotate_arg = if qsv_full_overlay || source_rotation_filter.is_some() {
        "-noautorotate"
    } else {
        "-autorotate"
    };
    input_0_args.extend([
        source_autorotate_arg.to_string(),
        "-i".to_string(),
        video_path.clone(),
    ]);

    // ── PHASE 4: BUILD INPUT 1 ARGS (raw atlas frames via stdin pipe) ──
    // Players display an untagged HD source as BT.709: tag the main video so
    // the overlay blends, and the output is labelled, in that colour space.
    let retagged_color = source_color.and_then(|color| color.untagged_hd_as_bt709(frame_size));
    let main_colorspace_tag = retagged_color.map(|_| "bt709");
    let source_color = retagged_color.or(source_color);
    let overlay_pixel_format = match source_color {
        Some(color) if !qsv_full_overlay => OverlayPixelFormat::Yuva420p(color),
        _ => OverlayPixelFormat::Rgba,
    };
    let mut input_1_args = vec!["-f".to_string(), "rawvideo".to_string()];
    input_1_args.extend(overlay_pixel_format.ffmpeg_input_args());
    let atlas_size = atlas.size();
    input_1_args.extend([
        "-s".to_string(),
        format!("{}x{}", atlas_size.width, atlas_size.height),
        "-r".to_string(),
        render.overlay_pipe_fps.ffmpeg_arg(),
        "-i".to_string(),
        "pipe:0".to_string(),
    ]);

    // ── PHASE 5: BUILD INPUT 2 ARGS (coarse seek plus filter-side audio trim) ──
    let mut input_2_args = Vec::new();
    if include_audio {
        if input_window.seek_start > 0.0 {
            input_2_args.extend([
                "-ss".to_string(),
                format_seconds_arg(input_window.seek_start),
            ]);
        }
        input_2_args.extend(["-i".to_string(), video_path]);
    }

    // ── PHASE 6: BUILD FILTER COMPLEX (video trim + scale + overlay + format) ──
    let mut filter_complex = composite_filter_complex(
        width,
        height,
        atlas,
        input_window.filter_start,
        render.render_duration,
        selected_profile,
        source_rotation_degrees,
        source_rotation_filter,
        qsv_overlay_cpu_rotation_filter,
        main_colorspace_tag,
    );
    if include_audio {
        filter_complex.push_str(&format!(
            ";[2:a]atrim=start={}:duration={},asetpts=N/SR/TB[aout]",
            format_seconds_arg(input_window.filter_start),
            format_seconds_arg(render.render_duration),
        ));
    }

    // ── PHASE 7: BUILD OUTPUT ARGS (map, codecs, bitrate, audio encode, mux flags) ──
    let mut output_args = vec!["-map".to_string(), "[out]".to_string()];
    if include_audio {
        output_args.extend(["-map".to_string(), "[aout]".to_string()]);
    }
    output_args.extend(["-r".to_string(), render.source_fps.ffmpeg_arg()]);
    output_args.extend([
        "-c:v".to_string(),
        selected_profile
            .codec_id
            .metadata()
            .encoder_id
            .metadata()
            .ffmpeg_name
            .to_string(),
    ]);
    output_args.extend(
        selected_profile
            .output_args
            .iter()
            .map(|arg| (*arg).to_string()),
    );
    if let Some(metadata_filter) =
        cuda_display_metadata_filter(selected_profile.codec_id, width, height)
    {
        output_args.extend(["-bsf:v".to_string(), metadata_filter]);
    }
    output_args.extend(rate_control_args(selected_profile, render.quality));
    if include_audio {
        output_args.extend([
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            audio_bit_rate.unwrap_or(192_000).to_string(),
        ]);
    }
    output_args.extend(["-movflags".to_string(), "faststart".to_string()]);
    // Physical-rotation profiles clear the now-stale display matrix. QSV-full
    // leaves it untouched so players rotate the completed coded composite.
    if !qsv_full_overlay {
        output_args.extend(["-metadata:s:v:0".to_string(), "rotate=0".to_string()]);
    }
    output_args.push("-y".to_string());

    Ok(CompositeFfmpegSettings {
        codec_id: selected_profile.codec_id,
        overlay_pixel_format,
        input_0_args,
        input_1_args,
        input_2_args,
        filter_complex,
        output_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::fps::Fps;
    use crate::render::SlotPlacement;

    fn assert_argument_pair(args: &[String], flag: &str, value: &str) {
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == flag && pair[1] == value),
            "missing argument pair {flag} {value}: {args:?}"
        );
    }

    fn render_plan(
        codec_id: CompositeCodecId,
        qsv_full_init_args: Vec<String>,
    ) -> CompositeRenderPlan {
        CompositeRenderPlan {
            frames: crate::encode::plan::FrameProductionPlan::new(30, std::num::NonZeroU32::MIN)
                .unwrap(),
            video_path: "rotated-landscape.mp4".into(),
            quality: crate::encode::quality::EncodingQuality::Bitrate(60.0),
            sync_offset: 0.0,
            trim_start: 0.0,
            render_duration: 1.0,
            update_rate: std::num::NonZeroU32::MIN,
            source_fps: Fps::new(30, 1).unwrap(),
            overlay_pipe_fps: Fps::new(30, 1).unwrap(),
            overlay_frame_count: 30,
            output_frame_count: 30,
            coverage: crate::encode::video_timing::ActivityCoverage::for_video(
                1.0,
                0.0,
                1.0,
                Fps::new(30, 1).unwrap(),
                30,
            )
            .unwrap(),
            requested_codec_id: codec_id,
            qsv_full_init_args,
        }
    }

    #[test]
    fn untagged_hd_sources_tag_the_main_video_bt709() {
        let filter_complex = |color, width, height| {
            let size = FrameSize { width, height };
            build_composite_ffmpeg_settings(
                &render_plan(CompositeCodecId::SoftwareHevc, Vec::new()),
                size,
                &AtlasPlan::full_frame(size, 0),
                false,
                None,
                None,
                color,
            )
            .unwrap()
            .filter_complex
        };
        let untagged = YuvColor::from_probe(None, Some("pc"));
        assert!(filter_complex(untagged, 3840, 2160)
            .starts_with("[0:v]setparams=colorspace=bt709,trim="));
        assert!(!filter_complex(untagged, 720, 576).contains("setparams"));
        let bt709 = YuvColor::from_probe(Some("bt709"), Some("tv"));
        assert!(!filter_complex(bt709, 1920, 1080).contains("setparams"));
    }

    #[test]
    fn overlays_pipe_yuva420p_in_the_source_colour_except_for_qsv_full_overlay() {
        let size = FrameSize {
            width: 1920,
            height: 1080,
        };
        let bt709 = YuvColor::from_probe(Some("bt709"), Some("tv"));
        let settings = |codec_id, init_args, color| {
            build_composite_ffmpeg_settings(
                &render_plan(codec_id, init_args),
                size,
                &AtlasPlan::full_frame(size, 0),
                false,
                None,
                None,
                color,
            )
            .unwrap()
        };

        let software = settings(CompositeCodecId::SoftwareHevc, Vec::new(), bt709);
        assert_eq!(
            software.overlay_pixel_format,
            OverlayPixelFormat::Yuva420p(bt709.unwrap())
        );
        assert_argument_pair(&software.input_1_args, "-pix_fmt", "yuva420p");
        assert_argument_pair(&software.input_1_args, "-colorspace", "bt709");
        assert_argument_pair(&software.input_1_args, "-color_range", "tv");

        // An untagged HD source's overlay is converted and tagged as BT.709.
        let untagged = YuvColor::from_probe(None, Some("pc"));
        let gopro = settings(CompositeCodecId::SoftwareHevc, Vec::new(), untagged);
        assert_argument_pair(&gopro.input_1_args, "-colorspace", "bt709");
        assert_argument_pair(&gopro.input_1_args, "-color_range", "pc");

        let unsupported = settings(CompositeCodecId::SoftwareHevc, Vec::new(), None);
        assert_eq!(unsupported.overlay_pixel_format, OverlayPixelFormat::Rgba);

        let qsv_init = ["-init_hw_device", "qsv=qs"].map(String::from).to_vec();
        let qsv_full = settings(CompositeCodecId::QsvFullHevc, qsv_init, bt709);
        assert_eq!(qsv_full.overlay_pixel_format, OverlayPixelFormat::Rgba);
        assert_argument_pair(&qsv_full.input_1_args, "-pix_fmt", "rgba");
    }

    #[test]
    fn rotated_cuda_profiles_use_post_orientation_dimensions_for_metadata() {
        for (codec_id, expected_metadata) in [
            (
                CompositeCodecId::NnvgpuH264,
                "h264_metadata=crop_right=8:crop_bottom=0",
            ),
            (
                CompositeCodecId::NnvgpuHevc,
                "hevc_metadata=width=1080:height=1920",
            ),
        ] {
            let render = render_plan(codec_id, Vec::new());
            let size = FrameSize {
                width: 1080,
                height: 1920,
            };
            let settings = build_composite_ffmpeg_settings(
                &render,
                size,
                &AtlasPlan::full_frame(size, 0),
                true,
                None,
                Some(90),
                None,
            )
            .unwrap();

            assert!(settings
                .filter_complex
                .contains("transpose_cuda=2,sidedata=mode=delete:type=DISPLAYMATRIX,scale_cuda"));
            assert_argument_pair(&settings.output_args, "-bsf:v", expected_metadata);
        }
    }

    const UHD: FrameSize = FrameSize {
        width: 3840,
        height: 2160,
    };

    fn planned_atlas() -> AtlasPlan {
        AtlasPlan::from_layer_bounds(
            UHD,
            &[
                Some(skia_safe::IRect::new(101, 200, 500, 301)),
                None,
                Some(skia_safe::IRect::new(3000, 1800, 3600, 2100)),
            ],
            SlotPlacement::Overlay,
        )
        .unwrap()
    }

    fn atlas_settings(codec_id: CompositeCodecId, atlas: &AtlasPlan) -> CoreResult<CompositeFfmpegSettings> {
        build_composite_ffmpeg_settings(
            &render_plan(codec_id, Vec::new()),
            UHD,
            atlas,
            true,
            None,
            None,
            YuvColor::from_probe(Some("bt709"), Some("tv")),
        )
    }

    #[test]
    fn cpu_overlay_graphs_crop_and_overlay_every_atlas_slot_at_its_scene_position() {
        let atlas = planned_atlas();
        assert_eq!(atlas.slots().len(), 2);
        for codec_id in [
            CompositeCodecId::SoftwareH264,
            CompositeCodecId::SoftwareHevc,
            CompositeCodecId::NvgpuH264,
            CompositeCodecId::NvgpuHevc,
            CompositeCodecId::QsvH264,
            CompositeCodecId::QsvHevc,
            CompositeCodecId::AmfH264,
            CompositeCodecId::AmfHevc,
            CompositeCodecId::MacH264,
            CompositeCodecId::MacHevc,
        ] {
            let settings = atlas_settings(codec_id, &atlas).unwrap();
            let graph = &settings.filter_complex;
            let name = codec_id.metadata().profile_name;
            assert_argument_pair(
                &settings.input_1_args,
                "-s",
                &format!("{}x{}", atlas.size().width, atlas.size().height),
            );
            assert!(graph.contains("[1:v]setpts=PTS-STARTPTS,split=2[atlas0][atlas1];"), "{name}: {graph}");
            assert_eq!(graph.matches("crop=").count(), 2, "{name}: {graph}");
            assert_eq!(graph.matches("overlay=").count(), 2, "{name}: {graph}");
            for (index, slot) in atlas.slots().iter().enumerate() {
                let rect = AtlasPlan::atlas_rect(slot);
                let crop = format!(
                    "[atlas{index}]crop={}:{}:{}:{}[slot{index}]",
                    rect.width(),
                    rect.height(),
                    rect.left,
                    rect.top
                );
                assert!(graph.contains(&crop), "{name}: {graph}");
                let overlay = format!(
                    "[slot{index}]overlay={}:{}:eof_action=repeat:shortest=1",
                    slot.scene.left, slot.scene.top
                );
                assert!(graph.contains(&overlay), "{name}: {graph}");
            }
            assert!(graph.contains("[base][slot0]overlay="), "{name}: {graph}");
            assert!(graph.contains("[main1];[main1][slot1]overlay="), "{name}: {graph}");
            assert!(graph.ends_with("[2:a]atrim=start=0:duration=1,asetpts=N/SR/TB[aout]"), "{name}: {graph}");
        }
    }

    #[test]
    fn single_slot_and_empty_atlases_keep_one_overlay() {
        let full = atlas_settings(CompositeCodecId::SoftwareH264, &AtlasPlan::full_frame(UHD, 2)).unwrap();
        assert!(full.filter_complex.contains(
            "[1:v]setpts=PTS-STARTPTS,crop=3840:2160:0:0[slot0];[base][slot0]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]"
        ));
        assert_argument_pair(&full.input_1_args, "-s", "3840x2160");

        let empty = AtlasPlan::from_layer_bounds(UHD, &[None], SlotPlacement::Overlay).unwrap();
        let empty = atlas_settings(CompositeCodecId::AmfHevc, &empty).unwrap();
        assert!(empty.filter_complex.contains(
            "[1:v]setpts=PTS-STARTPTS,null[slot0];[base][slot0]overlay=0:0:eof_action=repeat:shortest=1,format=nv12,hwupload[out]"
        ));
        assert_argument_pair(&empty.input_1_args, "-s", "2x2");
    }

    #[test]
    fn hardware_overlay_graphs_upload_and_overlay_every_atlas_slot_at_its_scene_position() {
        let atlas = planned_atlas();
        assert_eq!(atlas.slots().len(), 2);
        for (codec_id, base, overlay_filter) in [
            (CompositeCodecId::NnvgpuH264, "scale_cuda=w=3840:h=2160:format=yuv420p[base]", "overlay_cuda"),
            (CompositeCodecId::NnvgpuHevc, "scale_cuda=w=3840:h=2160:format=yuv420p[base]", "overlay_cuda"),
            (CompositeCodecId::VaapiH264, "scale_vaapi=w=3840:h=2160:format=nv12[base]", "overlay_vaapi"),
            (CompositeCodecId::VaapiHevc, "scale_vaapi=w=3840:h=2160:format=nv12[base]", "overlay_vaapi"),
        ] {
            let settings = atlas_settings(codec_id, &atlas).unwrap();
            let graph = &settings.filter_complex;
            let name = codec_id.metadata().profile_name;
            assert_argument_pair(
                &settings.input_1_args,
                "-s",
                &format!("{}x{}", atlas.size().width, atlas.size().height),
            );
            assert!(graph.contains(base), "{name}: {graph}");
            assert!(
                graph.contains("[1:v]setpts=PTS-STARTPTS,format=yuva420p,split=2[atlas0][atlas1];"),
                "{name}: {graph}"
            );
            assert_eq!(graph.matches("crop=").count(), 2, "{name}: {graph}");
            assert_eq!(graph.matches("hwupload").count(), 2, "{name}: {graph}");
            assert_eq!(graph.matches(&format!("{overlay_filter}=")).count(), 2, "{name}: {graph}");
            for (index, slot) in atlas.slots().iter().enumerate() {
                let rect = AtlasPlan::atlas_rect(slot);
                let crop = format!(
                    "[atlas{index}]crop={}:{}:{}:{},hwupload[slot{index}]",
                    rect.width(),
                    rect.height(),
                    rect.left,
                    rect.top
                );
                assert!(graph.contains(&crop), "{name}: {graph}");
                let (x, y) = (slot.scene.left, slot.scene.top);
                let coordinates = if overlay_filter == "overlay_vaapi" { format!("x={x}:y={y}") } else { format!("{x}:{y}") };
                let overlay = format!("[slot{index}]{overlay_filter}={coordinates}:eof_action=repeat:shortest=1");
                assert!(graph.contains(&overlay), "{name}: {graph}");
            }
            assert!(graph.contains(&format!("[base][slot0]{overlay_filter}=")), "{name}: {graph}");
            assert!(graph.contains(&format!("[main1];[main1][slot1]{overlay_filter}=")), "{name}: {graph}");
            assert!(graph.contains("shortest=1[out];[2:a]atrim="), "{name}: {graph}");
        }
    }

    #[test]
    fn hardware_overlay_graphs_upload_the_empty_atlas_whole() {
        let empty = AtlasPlan::from_layer_bounds(UHD, &[None], SlotPlacement::Overlay).unwrap();
        let cuda = atlas_settings(CompositeCodecId::NnvgpuHevc, &empty).unwrap();
        assert!(cuda.filter_complex.contains(
            "[1:v]setpts=PTS-STARTPTS,format=yuva420p,null,hwupload[slot0];[base][slot0]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]"
        ));
        let vaapi = atlas_settings(CompositeCodecId::VaapiH264, &empty).unwrap();
        assert!(vaapi.filter_complex.contains(
            "[1:v]setpts=PTS-STARTPTS,format=yuva420p,null,hwupload[slot0];[base][slot0]overlay_vaapi=x=0:y=0:eof_action=repeat:shortest=1[out]"
        ));
    }

    #[test]
    fn rotated_hardware_overlay_graphs_keep_gpu_rotation_and_overlay_atlas_slots() {
        let portrait = FrameSize {
            width: 1080,
            height: 1920,
        };
        let atlas = AtlasPlan::from_layer_bounds(
            portrait,
            &[
                Some(skia_safe::IRect::new(40, 60, 400, 200)),
                Some(skia_safe::IRect::new(600, 1500, 1000, 1800)),
            ],
            SlotPlacement::Overlay,
        )
        .unwrap();
        assert_eq!(atlas.slots().len(), 2);
        for (codec_id, rotation, overlay_filter) in [
            (CompositeCodecId::NnvgpuH264, "transpose_cuda=1,", "overlay_cuda="),
            (CompositeCodecId::VaapiHevc, "transpose_vaapi=1,", "overlay_vaapi="),
        ] {
            let settings = build_composite_ffmpeg_settings(
                &render_plan(codec_id, Vec::new()),
                portrait,
                &atlas,
                false,
                None,
                Some(270),
                None,
            )
            .unwrap();
            let graph = &settings.filter_complex;
            assert_eq!(settings.codec_id, codec_id);
            assert!(graph.contains(&format!("{rotation}sidedata=mode=delete:type=DISPLAYMATRIX,scale_")), "{graph}");
            assert_eq!(graph.matches(overlay_filter).count(), 2, "{graph}");
            assert_argument_pair(&settings.input_0_args, "-noautorotate", "-i");
        }
    }

    fn qsv_full_settings(frame: FrameSize, atlas: &AtlasPlan, rotation: Option<i32>) -> CompositeFfmpegSettings {
        let qsv_init = ["-init_hw_device", "qsv=qs"].map(String::from).to_vec();
        build_composite_ffmpeg_settings(
            &render_plan(CompositeCodecId::QsvFullHevc, qsv_init),
            frame,
            atlas,
            true,
            None,
            rotation,
            YuvColor::from_probe(Some("bt709"), Some("tv")),
        )
        .unwrap()
    }

    #[test]
    fn qsv_full_overlay_places_every_atlas_slot_at_its_coded_position_for_every_rotation() {
        let atlas = planned_atlas();
        let scenes: Vec<_> = atlas.slots().iter().map(|slot| slot.scene).collect();
        assert_eq!(
            scenes,
            [
                skia_safe::IRect::new(100, 200, 500, 302),
                skia_safe::IRect::new(3000, 1800, 3600, 2100)
            ]
        );
        for (rotation, main_scale, tile_rotation, positions) in [
            (None, "scale_qsv=w=3840:h=2160", "", [(100, 200), (3000, 1800)]),
            (Some(90), "scale_qsv=w=2160:h=3840", "transpose=1,", [(1858, 100), (60, 3000)]),
            (Some(180), "scale_qsv=w=3840:h=2160", "hflip,vflip,", [(3340, 1858), (240, 60)]),
            (Some(270), "scale_qsv=w=2160:h=3840", "transpose=2,", [(200, 3340), (1800, 240)]),
        ] {
            let settings = qsv_full_settings(UHD, &atlas, rotation);
            let graph = &settings.filter_complex;
            assert_argument_pair(
                &settings.input_1_args,
                "-s",
                &format!("{}x{}", atlas.size().width, atlas.size().height),
            );
            assert_argument_pair(&settings.input_1_args, "-pix_fmt", "rgba");
            assert!(graph.contains(&format!("{main_scale}:format=nv12[base]")), "{rotation:?}: {graph}");
            assert!(graph.contains("[1:v]setpts=PTS-STARTPTS,split=2[atlas0][atlas1];"), "{rotation:?}: {graph}");
            assert!(!graph.contains("sidedata=mode=delete"), "{rotation:?}: {graph}");
            for (index, slot) in atlas.slots().iter().enumerate() {
                let rect = AtlasPlan::atlas_rect(slot);
                let crop = format!(
                    "[atlas{index}]crop={}:{}:{}:{},{tile_rotation}hwupload=extra_hw_frames=16[slot{index}]",
                    rect.width(),
                    rect.height(),
                    rect.left,
                    rect.top
                );
                assert!(graph.contains(&crop), "{rotation:?}: {graph}");
            }
            let [(x0, y0), (x1, y1)] = positions;
            assert!(
                graph.contains(&format!(
                    "[base][slot0]overlay_qsv=x={x0}:y={y0}[main1];[main1][slot1]overlay_qsv=x={x1}:y={y1}[out]"
                )),
                "{rotation:?}: {graph}"
            );
        }
    }

    #[test]
    fn qsv_full_overlay_clips_slots_to_an_odd_frame_before_mapping_them() {
        let frame = FrameSize {
            width: 1919,
            height: 1079,
        };
        let atlas = AtlasPlan::from_layer_bounds(frame, &[Some(skia_safe::IRect::new(1900, 1000, 1919, 1079))], SlotPlacement::Overlay).unwrap();
        let slot = &atlas.slots()[0];
        assert_eq!(slot.scene, skia_safe::IRect::new(1900, 1000, 1920, 1080));
        let (left, top) = (slot.atlas_origin.x, slot.atlas_origin.y);
        for (rotation, position) in [(None, "x=1900:y=1000"), (Some(90), "x=0:y=1900"), (Some(180), "x=0:y=0"), (Some(270), "x=1000:y=0")] {
            let graph = qsv_full_settings(frame, &atlas, rotation).filter_complex;
            assert!(graph.contains(&format!("crop=19:79:{left}:{top},")), "{rotation:?}: {graph}");
            assert!(graph.contains(&format!("[base][slot0]overlay_qsv={position}[out]")), "{rotation:?}: {graph}");
        }
    }

    #[test]
    fn qsv_full_overlay_uploads_the_empty_atlas_whole() {
        let empty = AtlasPlan::from_layer_bounds(UHD, &[None], SlotPlacement::Overlay).unwrap();
        let graph = qsv_full_settings(UHD, &empty, Some(90)).filter_complex;
        assert!(graph.contains(
            "[1:v]setpts=PTS-STARTPTS,null,transpose=1,hwupload=extra_hw_frames=16[slot0];[base][slot0]overlay_qsv=x=0:y=0[out]"
        ));
    }
}
