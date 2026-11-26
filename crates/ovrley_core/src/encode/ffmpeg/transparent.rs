//! Transparent-overlay FFmpeg settings: codec arguments from the validated config, and the
//! filters that turn the piped atlas back into the full transparent frame.

use crate::encode::ffmpeg::catalog::{CodecSelection, TransparentCodecId};
use crate::encode::ffmpeg::profiles::transparent_profile;
use crate::error::{CoreError, CoreResult};
use crate::config::ValidatedFfmpegConfig;
use crate::render::{AtlasPlan, FrameSize};
use std::path::Path;
use skia_safe::IRect;

/// Fully resolved ffmpeg settings for one render.
#[derive(Clone, Debug)]
pub struct FfmpegSettings {
    pub codec_id: TransparentCodecId,
    /// ffmpeg loglevel passed to `-loglevel`.
    pub loglevel: String,
    /// Input-side hardware-device setup args required before rawvideo input.
    pub input_args: Vec<String>,
    /// Optional profile filter chain applied to the rebuilt full frame before encoding.
    pub filter_complex: Option<String>,
    /// Codec-specific output args appended before output path.
    pub output_args: Vec<String>,
    /// Public output file extension.
    pub extension: String,
}

impl FfmpegSettings {
    /// Produces the final argv at the FFmpeg process boundary. The input is the
    /// `atlas` frame, rebuilt into the transparent `frame_size` frame before the
    /// profile's filters.
    pub(crate) fn command_args(
        &self,
        output_path: &Path,
        atlas: &AtlasPlan,
        frame_size: FrameSize,
        fps: &str,
        input_pix_fmt: &str,
    ) -> Vec<String> {
        let mut args = vec!["-loglevel".to_string(), self.loglevel.clone()];
        args.extend(self.input_args.iter().cloned());
        args.extend([
            "-f".to_string(),
            "rawvideo".to_string(),
            "-s".to_string(),
            format!("{}x{}", atlas.size().width, atlas.size().height),
            "-pix_fmt".to_string(),
            input_pix_fmt.to_string(),
            "-r".to_string(),
            fps.to_string(),
            "-i".to_string(),
            "-".to_string(),
        ]);
        let filters = [transparent_atlas_filter(atlas, frame_size), self.filter_complex.clone()];
        let filters: Vec<String> = filters.into_iter().flatten().collect();
        if !filters.is_empty() {
            args.extend(["-vf".to_string(), filters.join(",")]);
        }
        args.extend(self.output_args.iter().cloned());
        args.extend(["-y".to_string(), output_path.to_string_lossy().into_owned()]);
        args
    }
}

/// Builds validated ffmpeg settings from `scene.ffmpeg`.
///
/// Supported codecs are alpha-preserving formats suitable for overlay exports.
/// Profile-specific FFmpeg defaults come from the transparent profile catalog.
/// The ingress validator ignores non-profile extension keys; `output_args`
/// remains the explicit escape hatch for advanced users.
pub fn build_ffmpeg_settings(ffmpeg_config: &ValidatedFfmpegConfig) -> CoreResult<FfmpegSettings> {
    let codec_id = match ffmpeg_config.codec {
        CodecSelection::Transparent(codec_id) => codec_id,
        CodecSelection::Composite(codec_id) => {
            return Err(CoreError::Encode(format!(
                "Composite codec '{}' cannot be used for a transparent render",
                codec_id.metadata().profile_name
            )))
        }
    };
    let profile = transparent_profile(codec_id);
    let encoder_name = codec_id.metadata().encoder_id.metadata().ffmpeg_name;
    let mut output_args = vec!["-c:v".to_string(), encoder_name.to_string()];
    output_args.extend(
        profile
            .output_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>(),
    );
    if let Some(container) = &ffmpeg_config.container {
        replace_arg_pair_value(&mut output_args, "-f", container);
    }
    if codec_id != TransparentCodecId::ProresKsVulkan {
        if let Some(pix_fmt) = ffmpeg_config.pix_fmt.as_deref() {
            replace_arg_pair_value(&mut output_args, "-pix_fmt", pix_fmt);
        }
    }
    output_args.extend(ffmpeg_config.output_args.iter().cloned());
    if codec_id == TransparentCodecId::ProresKsVulkan
        && !output_args.iter().any(|value| value == "-async_depth")
    {
        output_args.extend(["-async_depth".to_string(), "4".to_string()]);
    }

    Ok(FfmpegSettings {
        codec_id,
        loglevel: ffmpeg_config.loglevel.clone(),
        input_args: profile
            .input_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
        filter_complex: profile.filter_complex.map(str::to_string),
        output_args,
        extension: ffmpeg_config
            .container
            .clone()
            .unwrap_or_else(|| "mov".to_string()),
    })
}

/// Replaces one flag/value pair or appends it if the pair does not exist yet.
fn replace_arg_pair_value(args: &mut Vec<String>, flag: &str, value: &str) {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args[index + 1] = value.to_string();
        return;
    }

    args.push(flag.to_string());
    args.push(value.to_string());
}

/// Builds the filters that rebuild the transparent full frame from the atlas.
///
/// Slots are copied, never blended: a transparent plan places its slots with
/// [`crate::render::SlotPlacement::Copy`], so they never overlap. One `xstack`
/// places every slot in a single pass and `pad` fills the rest of the frame
/// with transparent pixels; chained overlays would copy the whole frame once
/// per slot. Returns `None` for the full-frame plan, whose atlas is the frame.
fn transparent_atlas_filter(atlas: &AtlasPlan, frame: FrameSize) -> Option<String> {
    let pad = format!("pad={}:{}", frame.width, frame.height);
    let frame_rect = IRect::from_wh(frame.width as i32, frame.height as i32);
    let placements: Vec<_> = atlas
        .slots()
        .iter()
        .map(|slot| AtlasPlan::frame_placement(slot, frame))
        .collect();
    match placements.as_slice() {
        [] => Some(format!("{pad}:0:0:color=black@0")),
        [(scene, crop)] if *scene == frame_rect && *crop == frame_rect => None,
        [(scene, crop)] => Some(format!(
            "crop={}:{}:{}:{},{pad}:{}:{}:color=black@0",
            crop.width(),
            crop.height(),
            crop.left,
            crop.top,
            scene.left,
            scene.top
        )),
        _ => {
            let mut graph = format!("split={}", placements.len());
            for index in 0..placements.len() {
                graph.push_str(&format!("[atlas{index}]"));
            }
            let mut layout = Vec::new();
            for (index, (scene, crop)) in placements.iter().enumerate() {
                graph.push_str(&format!(
                    ";[atlas{index}]crop={}:{}:{}:{}[slot{index}]",
                    crop.width(),
                    crop.height(),
                    crop.left,
                    crop.top
                ));
                layout.push(format!("{}_{}", scene.left, scene.top));
            }
            graph.push(';');
            for index in 0..placements.len() {
                graph.push_str(&format!("[slot{index}]"));
            }
            graph.push_str(&format!(
                "xstack=inputs={}:layout={}:fill=black@0,{pad}:0:0:color=black@0",
                placements.len(),
                layout.join("|")
            ));
            Some(graph)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::SlotPlacement;

    const HD: FrameSize = FrameSize {
        width: 1920,
        height: 1080,
    };

    #[test]
    fn full_frame_plan_needs_no_rebuild() {
        assert_eq!(transparent_atlas_filter(&AtlasPlan::full_frame(HD, 2), HD), None);
    }

    #[test]
    fn empty_plan_pads_the_transparent_atlas_to_the_frame() {
        let atlas = AtlasPlan::from_layer_bounds(HD, &[None], SlotPlacement::Copy).unwrap();
        assert_eq!(
            transparent_atlas_filter(&atlas, HD).unwrap(),
            "pad=1920:1080:0:0:color=black@0"
        );
    }

    #[test]
    fn one_slot_is_cropped_and_padded_into_place() {
        let atlas = AtlasPlan::from_layer_bounds(HD, &[Some(IRect::new(100, 200, 300, 260))], SlotPlacement::Copy).unwrap();
        let slot = &atlas.slots()[0];
        assert_eq!(
            transparent_atlas_filter(&atlas, HD).unwrap(),
            format!(
                "crop=200:60:{}:{},pad=1920:1080:100:200:color=black@0",
                slot.atlas_origin.x, slot.atlas_origin.y
            )
        );
    }

    #[test]
    fn slots_are_stacked_at_their_scene_positions_clipped_to_the_frame() {
        let frame = FrameSize {
            width: 1281,
            height: 721,
        };
        let atlas = AtlasPlan::from_layer_bounds(
            frame,
            &[Some(IRect::new(10, 20, 110, 60)), Some(IRect::new(1201, 701, 1281, 721))],
            SlotPlacement::Copy,
        )
        .unwrap();
        let [first, second] = atlas.slots() else {
            panic!("two slots: {atlas:?}");
        };
        let graph = transparent_atlas_filter(&atlas, frame).unwrap();
        assert_eq!(
            graph,
            format!(
                "split=2[atlas0][atlas1];[atlas0]crop=100:40:{}:{}[slot0];[atlas1]crop=81:21:{}:{}[slot1];\
                 [slot0][slot1]xstack=inputs=2:layout=10_20|1200_700:fill=black@0,pad=1281:721:0:0:color=black@0",
                first.atlas_origin.x, first.atlas_origin.y, second.atlas_origin.x, second.atlas_origin.y
            )
        );
    }
}
