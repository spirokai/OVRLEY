//! Atlas plan: where each widget's tile lives in the piped overlay frame.
//!
//! A planning pass unions every widget's layout bounds over all planned export
//! frames into one slot per widget. Slots are snapped outward to even scene
//! coordinates and sizes, because `yuva420p` chroma covers 2×2 blocks.
//! Overlapping slots are merged when their union costs no more than the two
//! slots apart, or when their layers interleave; widgets inside one slot
//! composite in layer order. Slots left overlapping are overlaid in layer
//! order, since compositing over is associative. A plan whose slots are copied
//! onto a transparent frame instead merges every overlap. The slots are then
//! shelf-packed into one atlas frame. Each slot
//! keeps its scene position modulo [`PIXEL_PHASE`], so widgets drawn straight
//! into the atlas produce the pixels they produce in the full frame, up to one
//! level where Skia's float anti-aliasing rounds a shifted path differently.
//!
//! The full frame is the plan with one slot covering the frame. The plan is
//! immutable for an export and is the single source for frame production and
//! the FFmpeg graph.

use super::FrameSize;
use crate::error::{CoreError, CoreResult};
use skia_safe::{IPoint, IRect};

/// Skia's ordered dither repeats every 8 pixels, so drawing at an offset that is a
/// multiple of 8 reproduces the same dithered gradients.
pub const PIXEL_PHASE: i32 = 8;

/// Smallest position at or after `at` with the same phase as `scene`; even inputs stay even.
fn in_phase(at: i32, scene: i32) -> i32 {
    at + (scene - at).rem_euclid(PIXEL_PHASE)
}

/// How FFmpeg places the atlas slots back on the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotPlacement {
    /// Slots are overlaid in order onto the main video, so they may overlap.
    Overlay,
    /// Slots are copied onto a transparent frame, so they never overlap.
    Copy,
}

/// One region of the scene copied into the atlas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasSlot {
    /// Scene rectangle the slot covers. Planned slots are even-aligned and may
    /// extend one column or row past an odd frame edge.
    pub scene: IRect,
    /// Top-left corner of the slot in the atlas frame.
    pub atlas_origin: IPoint,
    /// Widget layers drawn into the slot, in layer order.
    pub layers: Vec<usize>,
}

/// Slots and the size of the atlas frame they are packed into. Slots are ordered
/// by their first layer, and the layers of overlapping slots never interleave,
/// so overlaying the slots in order composites every layer in layer order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasPlan {
    size: FrameSize,
    slots: Vec<AtlasSlot>,
}

impl AtlasPlan {
    /// One slot covering the frame and every layer: the atlas is the full frame.
    pub fn full_frame(frame: FrameSize, layer_count: usize) -> Self {
        Self {
            size: frame,
            slots: vec![AtlasSlot {
                scene: frame_rect(frame),
                atlas_origin: IPoint::new(0, 0),
                layers: (0..layer_count).collect(),
            }],
        }
    }

    /// Plans slots from each layer's union of frame-clipped bounds over every
    /// planned frame; layers without bounds are never visible and get no slot.
    /// `placement` decides which overlapping slots merge.
    /// A packed atlas at least as large as the frame degrades to the full frame.
    pub fn from_layer_bounds(frame: FrameSize, layer_bounds: &[Option<IRect>], placement: SlotPlacement) -> CoreResult<Self> {
        let limit = frame_rect(frame);
        let mut slots: Vec<(IRect, Vec<usize>)> = Vec::new();
        for (layer, bounds) in layer_bounds.iter().enumerate() {
            let Some(bounds) = bounds else { continue };
            if bounds.is_empty() || !contains(&limit, bounds) {
                return Err(CoreError::Render(format!(
                    "Atlas layer {layer} bounds {bounds:?} must be non-empty and inside the {}x{} frame",
                    frame.width, frame.height
                )));
            }
            slots.push((even_outward(bounds), vec![layer]));
        }
        merge_overlapping(&mut slots, placement);
        let plan = pack(slots);
        let area = |size: FrameSize| u64::from(size.width) * u64::from(size.height);
        Ok(if area(plan.size) >= area(frame) {
            Self::full_frame(frame, layer_bounds.len())
        } else {
            plan
        })
    }

    /// Size of the atlas frame piped to FFmpeg.
    pub fn size(&self) -> FrameSize {
        self.size
    }

    pub fn slots(&self) -> &[AtlasSlot] {
        &self.slots
    }

    /// Scene rectangle of `slot` clipped to `frame`, and the atlas rectangle
    /// holding it. A planned slot can extend one column or row past an odd frame edge.
    pub fn frame_placement(slot: &AtlasSlot, frame: FrameSize) -> (IRect, IRect) {
        let scene = IRect::new(
            slot.scene.left,
            slot.scene.top,
            slot.scene.right.min(frame.width as i32),
            slot.scene.bottom.min(frame.height as i32),
        );
        let atlas = IRect::from_xywh(slot.atlas_origin.x, slot.atlas_origin.y, scene.width(), scene.height());
        (scene, atlas)
    }

    /// Copies the slots of an RGBA atlas frame onto a transparent RGBA frame;
    /// slots of a [`SlotPlacement::Copy`] plan never overlap.
    pub fn unpack_rgba(&self, atlas: &[u8], frame: FrameSize) -> CoreResult<Vec<u8>> {
        let mut pixels = vec![0; frame.rgba_len()?];
        let (frame_stride, atlas_stride) = (frame.width as usize * 4, self.size.width as usize * 4);
        for slot in &self.slots {
            let (scene, rect) = Self::frame_placement(slot, frame);
            let row_bytes = scene.width() as usize * 4;
            for row in 0..scene.height() as usize {
                let from = (rect.top as usize + row) * atlas_stride + rect.left as usize * 4;
                let to = (scene.top as usize + row) * frame_stride + scene.left as usize * 4;
                pixels[to..to + row_bytes].copy_from_slice(&atlas[from..from + row_bytes]);
            }
        }
        Ok(pixels)
    }

    /// Atlas rectangle of `slot`.
    pub fn atlas_rect(slot: &AtlasSlot) -> IRect {
        IRect::from_xywh(
            slot.atlas_origin.x,
            slot.atlas_origin.y,
            slot.scene.width(),
            slot.scene.height(),
        )
    }
}

fn frame_rect(frame: FrameSize) -> IRect {
    IRect::from_wh(frame.width as i32, frame.height as i32)
}

fn contains(outer: &IRect, inner: &IRect) -> bool {
    outer.left <= inner.left
        && outer.top <= inner.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
}

fn overlaps(a: &IRect, b: &IRect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn union(a: &IRect, b: &IRect) -> IRect {
    IRect::new(
        a.left.min(b.left),
        a.top.min(b.top),
        a.right.max(b.right),
        a.bottom.max(b.bottom),
    )
}

/// Bounds are non-negative, so even snapping rounds left and top down and right and bottom up.
fn even_outward(rect: &IRect) -> IRect {
    IRect::new(
        rect.left & !1,
        rect.top & !1,
        rect.right + (rect.right & 1),
        rect.bottom + (rect.bottom & 1),
    )
}

fn area(rect: &IRect) -> i64 {
    i64::from(rect.width()) * i64::from(rect.height())
}

/// Whether neither slot's layers all lie below the other's; sorted layers assumed.
fn interleave(a: &[usize], b: &[usize]) -> bool {
    !(a[a.len() - 1] < b[0] || b[b.len() - 1] < a[0])
}

/// Merges overlapping slots until every remaining overlap is cheaper apart and
/// has layers that do not interleave. Apart, the overlap is blended twice;
/// merged, the union's empty area is piped and blended instead, so the union is
/// taken when it is no larger than the two slots together. Interleaved layers
/// have no overlay order and always merge. Copied slots have no blending and
/// always merge. A merged slot keeps its layers in layer order.
fn merge_overlapping(slots: &mut Vec<(IRect, Vec<usize>)>, placement: SlotPlacement) {
    'merge: loop {
        for first in 0..slots.len() {
            for second in first + 1..slots.len() {
                let ((a, a_layers), (b, b_layers)) = (&slots[first], &slots[second]);
                if overlaps(a, b)
                    && (placement == SlotPlacement::Copy
                        || area(&union(a, b)) <= area(a) + area(b)
                        || interleave(a_layers, b_layers))
                {
                    let (rect, layers) = slots.swap_remove(second);
                    slots[first].0 = union(&slots[first].0, &rect);
                    slots[first].1.extend(layers);
                    slots[first].1.sort_unstable();
                    continue 'merge;
                }
            }
        }
        break;
    }
    slots.sort_unstable_by_key(|(_, layers)| layers[0]);
}

/// Shelf packing, tallest slots first, into a roughly square atlas no narrower than the widest slot.
fn pack(slots: Vec<(IRect, Vec<usize>)>) -> AtlasPlan {
    if slots.is_empty() {
        // Nothing is ever visible; the smallest yuva420p frame stays transparent.
        return AtlasPlan {
            size: FrameSize { width: 2, height: 2 },
            slots: Vec::new(),
        };
    }
    let area: u64 = slots
        .iter()
        .map(|(rect, _)| rect.width() as u64 * rect.height() as u64)
        .sum();
    let widest = slots.iter().map(|(rect, _)| rect.width()).max().expect("slots");
    let square = ((area as f64).sqrt().ceil() as i32 + 1) & !1;
    let shelf_width = widest.max(square);
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|index| {
        let rect = slots[*index].0;
        (std::cmp::Reverse(rect.height()), std::cmp::Reverse(rect.width()), *index)
    });
    let mut origins = vec![IPoint::new(0, 0); slots.len()];
    let (mut x, mut y, mut shelf_height, mut width) = (0, 0, 0, 0);
    for index in order {
        let rect = slots[index].0;
        let mut left = in_phase(x, rect.left);
        if x > 0 && left + rect.width() > shelf_width {
            y += shelf_height;
            (x, shelf_height) = (0, 0);
            left = in_phase(x, rect.left);
        }
        let top = in_phase(y, rect.top);
        origins[index] = IPoint::new(left, top);
        x = left + rect.width();
        width = width.max(x);
        shelf_height = shelf_height.max(top - y + rect.height());
    }
    AtlasPlan {
        size: FrameSize {
            width: width as u32,
            height: (y + shelf_height) as u32,
        },
        slots: slots
            .into_iter()
            .zip(origins)
            .map(|((scene, layers), atlas_origin)| AtlasSlot {
                scene,
                atlas_origin,
                layers,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UHD: FrameSize = FrameSize {
        width: 3840,
        height: 2160,
    };

    fn assert_packed(plan: &AtlasPlan) {
        let size = plan.size();
        assert!(size.width % 2 == 0 && size.height % 2 == 0, "atlas {size:?} must be even");
        let atlas = frame_rect(size);
        for (index, slot) in plan.slots().iter().enumerate() {
            let rect = AtlasPlan::atlas_rect(slot);
            for value in [slot.scene.left, slot.scene.top, slot.scene.width(), slot.scene.height()] {
                assert_eq!(value % 2, 0, "slot {slot:?} must be even");
            }
            assert_eq!(
                (slot.atlas_origin.x - slot.scene.left).rem_euclid(PIXEL_PHASE),
                0,
                "slot {slot:?} keeps its horizontal pixel phase"
            );
            assert_eq!(
                (slot.atlas_origin.y - slot.scene.top).rem_euclid(PIXEL_PHASE),
                0,
                "slot {slot:?} keeps its vertical pixel phase"
            );
            assert!(contains(&atlas, &rect), "slot {slot:?} lies outside the atlas {size:?}");
            for other in &plan.slots()[index + 1..] {
                assert!(slot.layers[0] < other.layers[0], "slots {slot:?} and {other:?} are in layer order");
                assert!(
                    !overlaps(&slot.scene, &other.scene) || slot.layers[slot.layers.len() - 1] < other.layers[0],
                    "overlapping scene slots {slot:?} and {other:?} interleave layers"
                );
                assert!(
                    !overlaps(&rect, &AtlasPlan::atlas_rect(other)),
                    "atlas slots {slot:?} and {other:?} overlap"
                );
            }
        }
    }

    #[test]
    fn full_frame_plan_is_one_slot_covering_the_frame_with_every_layer() {
        let plan = AtlasPlan::full_frame(UHD, 3);
        assert_eq!(plan.size(), UHD);
        assert_eq!(
            plan.slots(),
            [AtlasSlot {
                scene: IRect::from_wh(3840, 2160),
                atlas_origin: IPoint::new(0, 0),
                layers: vec![0, 1, 2],
            }]
        );
    }

    #[test]
    fn slots_snap_outward_to_even_scene_rectangles_and_skip_invisible_layers() {
        let bounds = [
            Some(IRect::new(101, 51, 203, 98)),
            None,
            Some(IRect::new(3001, 2001, 3839, 2159)),
        ];
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        let scenes: Vec<_> = plan.slots().iter().map(|slot| (slot.scene, slot.layers.clone())).collect();
        assert_eq!(
            scenes,
            [
                (IRect::new(100, 50, 204, 98), vec![0]),
                (IRect::new(3000, 2000, 3840, 2160), vec![2]),
            ]
        );
    }

    #[test]
    fn odd_frame_edges_extend_the_even_slot_past_the_frame() {
        let frame = FrameSize { width: 1281, height: 721 };
        let plan = AtlasPlan::from_layer_bounds(frame, &[Some(IRect::new(1201, 701, 1281, 721))], SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        assert_eq!(plan.slots()[0].scene, IRect::new(1200, 700, 1282, 722));
    }

    #[test]
    fn overlapping_slots_merge_when_their_union_is_no_larger_than_the_slots_apart() {
        let bounds = [
            // 0 and 2 overlap almost entirely; 1 only touches the merged slot's right edge.
            Some(IRect::new(0, 0, 100, 100)),
            Some(IRect::new(110, 0, 150, 40)),
            Some(IRect::new(10, 10, 110, 110)),
        ];
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        let scenes: Vec<_> = plan.slots().iter().map(|slot| (slot.scene, slot.layers.clone())).collect();
        assert_eq!(
            scenes,
            [
                (IRect::new(0, 0, 110, 110), vec![0, 2]),
                (IRect::new(110, 0, 150, 40), vec![1]),
            ]
        );
    }

    #[test]
    fn costly_overlaps_stay_apart_in_layer_order() {
        // A wide strip crossed by a tall strip: their union is mostly empty.
        let bounds = [Some(IRect::new(0, 900, 3000, 1000)), Some(IRect::new(1400, 0, 1600, 2000))];
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        let scenes: Vec<_> = plan.slots().iter().map(|slot| (slot.scene, slot.layers.clone())).collect();
        assert_eq!(scenes, [(bounds[0].unwrap(), vec![0]), (bounds[1].unwrap(), vec![1])]);
        assert!(overlaps(&plan.slots()[0].scene, &plan.slots()[1].scene));
    }

    #[test]
    fn copied_slots_merge_every_overlap() {
        let bounds = [Some(IRect::new(0, 900, 3000, 1000)), Some(IRect::new(1400, 0, 1600, 2000))];
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Copy).unwrap();
        assert_packed(&plan);
        let scenes: Vec<_> = plan.slots().iter().map(|slot| (slot.scene, slot.layers.clone())).collect();
        assert_eq!(scenes, [(IRect::new(0, 0, 3000, 2000), vec![0, 1])]);
    }

    #[test]
    fn overlapping_slots_with_interleaved_layers_merge_however_costly() {
        let bounds = [
            Some(IRect::new(0, 0, 100, 100)),
            // Touches layer 0 only, but crosses the slot that 0 and 2 merge into.
            Some(IRect::new(100, 0, 2000, 40)),
            Some(IRect::new(10, 10, 110, 110)),
        ];
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        let scenes: Vec<_> = plan.slots().iter().map(|slot| (slot.scene, slot.layers.clone())).collect();
        assert_eq!(scenes, [(IRect::new(0, 0, 2000, 110), vec![0, 1, 2])]);
    }

    #[test]
    fn packs_many_slots_without_overlap_into_an_even_atlas_smaller_than_the_frame() {
        let bounds: Vec<_> = (0..40)
            .map(|index| {
                let (column, row) = (index % 8, index / 8);
                let (left, top) = (column * 470 + index % 3, row * 420 + index % 5);
                Some(IRect::from_xywh(left, top, 37 + index * 11 % 300, 23 + index * 7 % 200))
            })
            .collect();
        let plan = AtlasPlan::from_layer_bounds(UHD, &bounds, SlotPlacement::Overlay).unwrap();
        assert_packed(&plan);
        assert_eq!(plan.slots().len(), 40);
        let mut layers: Vec<_> = plan.slots().iter().flat_map(|slot| slot.layers.clone()).collect();
        layers.sort_unstable();
        assert_eq!(layers, (0..40).collect::<Vec<_>>());
        let atlas = u64::from(plan.size().width) * u64::from(plan.size().height);
        assert!(atlas * 4 < 3840 * 2160, "atlas {:?} must stay small", plan.size());
    }

    #[test]
    fn layouts_covering_the_frame_degrade_to_the_full_frame_plan() {
        let frame = FrameSize { width: 1920, height: 1080 };
        let bounds = [Some(IRect::new(0, 0, 1920, 1080)), Some(IRect::new(10, 10, 20, 20)), None];
        assert_eq!(AtlasPlan::from_layer_bounds(frame, &bounds, SlotPlacement::Overlay).unwrap(), AtlasPlan::full_frame(frame, 3));
    }

    #[test]
    fn without_visible_layers_the_atlas_is_one_transparent_chroma_block() {
        let plan = AtlasPlan::from_layer_bounds(UHD, &[None, None], SlotPlacement::Overlay).unwrap();
        assert_eq!(plan.size(), FrameSize { width: 2, height: 2 });
        assert!(plan.slots().is_empty());
    }

    #[test]
    fn unpacking_copies_each_slot_to_its_scene_position_clipped_to_the_frame() {
        let frame = FrameSize { width: 63, height: 31 };
        let plan = AtlasPlan::from_layer_bounds(
            frame,
            &[Some(IRect::new(0, 0, 2, 2)), Some(IRect::new(50, 20, 63, 31))],
            SlotPlacement::Copy,
        )
        .unwrap();
        assert_eq!(plan.slots().len(), 2, "{plan:?}");
        let atlas_size = plan.size();
        let atlas: Vec<u8> = (0..atlas_size.rgba_len().unwrap()).map(|byte| (byte / 4 + 1) as u8).collect();
        let pixels = plan.unpack_rgba(&atlas, frame).unwrap();
        let pixel = |x: usize, y: usize| pixels[(y * 63 + x) * 4];
        let atlas_pixel = |x: i32, y: i32| atlas[(y as usize * atlas_size.width as usize + x as usize) * 4];
        for slot in plan.slots() {
            let (scene, rect) = AtlasPlan::frame_placement(slot, frame);
            for (dy, dx) in (0..scene.height()).flat_map(|dy| (0..scene.width()).map(move |dx| (dy, dx))) {
                assert_eq!(
                    pixel((scene.left + dx) as usize, (scene.top + dy) as usize),
                    atlas_pixel(rect.left + dx, rect.top + dy)
                );
            }
        }
        assert_eq!(pixel(30, 10), 0, "pixels outside the slots stay transparent");
        assert_eq!(pixels.len(), frame.rgba_len().unwrap());
    }

    #[test]
    fn bounds_outside_the_frame_are_rejected() {
        for bounds in [IRect::new(-1, 0, 10, 10), IRect::new(0, 0, 3841, 10), IRect::new(5, 5, 5, 10)] {
            assert!(AtlasPlan::from_layer_bounds(UHD, &[Some(bounds)], SlotPlacement::Overlay).is_err(), "{bounds:?}");
        }
    }
}
