//! Transparent filter-graph helpers.
//!
//! Owns: the filters that turn the piped atlas back into the transparent full
//!       frame before a transparent profile's own filters.
//! Does not own: atlas planning or the profile filters themselves.

use crate::render::{AtlasPlan, FrameSize};
use skia_safe::IRect;

/// Builds the filters that rebuild the transparent full frame from the atlas.
///
/// Slots are copied, never blended: a transparent plan places its slots with
/// [`crate::render::SlotPlacement::Copy`], so they never overlap. One `xstack`
/// places every slot in a single pass and `pad` fills the rest of the frame
/// with transparent pixels; chained overlays would copy the whole frame once
/// per slot. Returns `None` for the full-frame plan, whose atlas is the frame.
pub(super) fn transparent_atlas_filter(atlas: &AtlasPlan, frame: FrameSize) -> Option<String> {
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
