//! Pointer interaction on the canvas: selection, the marquee and hover (D12, ports of
//! `useWidgetSelection` and `selectionGeometry.js`), and the transform (D13, ports of
//! `useWasmWidgetInteractions`, `widgetInteraction.js` and the Moveable behaviour in
//! `moveable-behaviour.md`).
//!
//! One pointer area covers the stage. A press is tested against the selection's handles, then the
//! widgets' rotated content boxes (topmost layer first), then it starts a marquee. A gesture keeps
//! its start state; each move replaces its intent and resolves patches from the start, never
//! accumulating. Patches preview as gesture patches (D7); release commits them as one action, so
//! one undo step.

use super::geometry::{
    group_bounds, guidelines, guides, resize_anchor, resize_geometry, rotates_anchor,
    rotation_patch, snap_axis, ContentBox, Guide, Guideline, Rect,
};
use super::{display_scale, editor_grid_size, scene_size, stage};
use crate::features::widget_preview::{preview_widgets, widget_layer, PreviewWidget};
use crate::store::Store;
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::{normalize_selection_ids, number, primary_selection_id};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The pointer modifiers, as the browser reports them (⌘ is `meta` on macOS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
}

impl PointerModifiers {
    /// `hasSelectionModifier`.
    pub fn selects(self) -> bool {
        self.meta || self.ctrl || self.shift
    }
}

/// A transform handle: a resize direction, or the rotation knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Resize { direction: (f64, f64) },
    Rotate,
}

/// The corner handles, then the edge handles (`CORNER_RESIZE_DIRECTIONS`, `EDGE_RESIZE_DIRECTIONS`).
const CORNERS: [(f64, f64); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
const EDGES: [(f64, f64); 4] = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];

/// `getWidgetResizeOptions`: what the selection may do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResizeOptions {
    pub can_drag: bool,
    pub can_resize: bool,
    pub can_rotate: bool,
    pub keep_ratio: bool,
    pub edge_handles: bool,
}

pub fn resize_options(widget: Option<&PreviewWidget>, group: bool) -> ResizeOptions {
    let Some(widget) = widget else {
        return ResizeOptions::default();
    };
    if group {
        return ResizeOptions {
            can_drag: true,
            can_resize: true,
            can_rotate: false,
            keep_ratio: true,
            edge_handles: false,
        };
    }
    let display = widget.data.get("display_type").and_then(Value::as_str);
    let shape = widget.data.get("shape").and_then(Value::as_str);
    ResizeOptions {
        can_drag: true,
        can_resize: true,
        can_rotate: widget.kind == "course" || widget.kind == "raster",
        edge_handles: matches!(
            widget.kind.as_str(),
            "raster" | "elevation" | "diving_profile" | "rolling_chart"
        ) || (widget.kind == "map" && shape == Some("rectangle")),
        keep_ratio: matches!(
            display,
            Some("arc" | "corner" | "gt_gauge" | "lean_angle" | "heading_compass" | "g_force")
        ) || (widget.kind == "backdrop" && display == Some("circle"))
            || (widget.kind == "map" && shape == Some("circle"))
            || widget.kind == "course"
            || !catalog().is_framed(&widget.kind, display),
    }
}

/// One widget's state at a gesture's start.
#[derive(Clone, Debug)]
struct Origin {
    kind: String,
    data: Map<String, Value>,
    content: ContentBox,
    scene: Value,
}

#[derive(Clone, Debug, PartialEq)]
enum GestureKind {
    Drag,
    Resize { direction: (f64, f64) },
    Rotate,
}

/// A running transform.
#[derive(Clone, Debug)]
pub struct Gesture {
    kind: GestureKind,
    ids: Vec<String>,
    group: bool,
    keep_ratio: bool,
    start: (f64, f64),
    origins: BTreeMap<String, Origin>,
    /// The box the gesture moves: the widget's content box, or the group's bounds.
    start_box: ContentBox,
    lines: Vec<Guideline>,
    /// `dragTranslation`'s axis lock.
    axis: Option<bool>,
    /// The rotation's unwrapped pointer angle.
    last_angle: Option<f64>,
    turned: f64,
    /// The last resolved patches, and whether any move happened.
    patches: BTreeMap<String, Map<String, Value>>,
    moved: bool,
    pub guides: Vec<Guide>,
    /// The document and selection the gesture started on; a change to either cancels it.
    config: *const ovrley_widgets::model::document::OverlayConfig,
    selection: Vec<String>,
}

impl Gesture {
    /// Whether the preview may reuse a widget's pixels at fractional positions.
    pub fn is_transient(&self, id: &str, kind: &str) -> bool {
        self.ids.iter().any(|candidate| candidate == id)
            && match self.kind {
                GestureKind::Drag => true,
                GestureKind::Resize { .. } => {
                    matches!(kind, "course" | "elevation" | "diving_profile")
                }
                GestureKind::Rotate => false,
            }
    }

    pub fn is_group(&self) -> bool {
        self.group
    }
}

/// The marquee (`marqueeRef`): scene and stage start points, the base selection, and its preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Marquee {
    additive: bool,
    base_ids: Vec<String>,
    preview_ids: Vec<String>,
    start_scene: (f64, f64),
    start_stage: (f64, f64),
    pub has_moved: bool,
    /// The rectangle in stage pixels.
    pub stage_rect: Rect,
}

/// What a press started.
#[derive(Clone, Debug, Default)]
pub enum PointerState {
    #[default]
    Idle,
    Marquee(Marquee),
    Gesture(Box<Gesture>),
}

/// The presented content boxes of the document's widgets, in document order.
fn content_boxes(store: &Store) -> Vec<(String, String, ContentBox)> {
    let presented = store.preview.presented();
    store
        .editor
        .config
        .widget_ids()
        .into_iter()
        .filter_map(|id| {
            presented
                .get(&id)
                .map(|widget| (id, widget.kind.clone(), ContentBox::of(&widget.geometry)))
        })
        .collect()
}

/// The topmost widget under a scene point: the highest layer, then the latest in the document.
pub fn hit_widget(store: &Store, point: (f64, f64)) -> Option<String> {
    let mut boxes: Vec<(usize, String, String, ContentBox)> = content_boxes(store)
        .into_iter()
        .enumerate()
        .map(|(order, (id, kind, content))| (order, id, kind, content))
        .collect();
    boxes.sort_by_key(|(order, _, kind, _)| (widget_layer(kind), *order));
    boxes
        .into_iter()
        .rev()
        .find(|(_, _, _, content)| content.contains(point))
        .map(|(_, id, _, _)| id)
}

/// Stage coordinates to scene coordinates.
pub fn stage_to_scene(store: &Store, point: (f64, f64)) -> (f64, f64) {
    let layout = stage(store);
    let scale = display_scale(store);
    (
        (point.0 - layout.scene_left) / scale,
        (point.1 - layout.scene_top) / scale,
    )
}

/// Scene coordinates to stage coordinates.
pub fn scene_to_stage(store: &Store, point: (f64, f64)) -> (f64, f64) {
    let layout = stage(store);
    let scale = display_scale(store);
    (
        layout.scene_left + point.0 * scale,
        layout.scene_top + point.1 * scale,
    )
}

/// The selection as the transform sees it: during a group drag, the dragged ids.
fn effective_selection(store: &Store) -> Vec<String> {
    match &store.overlay_editor.pointer {
        PointerState::Gesture(gesture) if gesture.group && gesture.kind == GestureKind::Drag => {
            gesture.ids.clone()
        }
        _ => normalize_selection_ids(
            &store.editor.selected_widget_ids,
            &store.editor.config.widget_ids(),
        ),
    }
}

/// The handles of the selection, in scene coordinates, with what each does.
pub fn handles(store: &Store) -> Vec<((f64, f64), Handle)> {
    let ids = effective_selection(store);
    let Some(theme) = store.theme.clone() else {
        return Vec::new();
    };
    let widgets = preview_widgets(store, &theme);
    let presented = store.preview.presented();
    let group = ids.len() > 1;
    let primary = primary_selection_id(&ids, store.editor.selected_widget_id.as_deref());
    let primary_widget = primary
        .as_ref()
        .and_then(|id| widgets.iter().find(|widget| &widget.id == id));
    let options = resize_options(primary_widget, group);
    if !options.can_resize {
        return Vec::new();
    }
    let content = if group {
        let boxes: Vec<ContentBox> = ids
            .iter()
            .filter_map(|id| {
                presented
                    .get(id)
                    .map(|widget| ContentBox::of(&widget.geometry))
            })
            .collect();
        if boxes.is_empty() {
            return Vec::new();
        }
        ContentBox {
            rect: group_bounds(&boxes),
            rotation: 0.0,
            origin: (0.0, 0.0),
        }
    } else {
        let Some(widget) = primary.as_ref().and_then(|id| presented.get(id)) else {
            return Vec::new();
        };
        ContentBox::of(&widget.geometry)
    };
    let mut list: Vec<((f64, f64), Handle)> = CORNERS
        .iter()
        .chain(if options.edge_handles {
            EDGES.iter()
        } else {
            [].iter()
        })
        .map(|direction| {
            (
                content.point((direction.0 + 1.0) / 2.0, (direction.1 + 1.0) / 2.0),
                Handle::Resize {
                    direction: *direction,
                },
            )
        })
        .collect();
    if options.can_rotate {
        list.push((rotation_knob(store, &content), Handle::Rotate));
    }
    list
}

/// The rotation knob: 40 Moveable pixels (×0.75×handle scale on screen) out from the top edge's
/// middle, perpendicular to it.
pub fn rotation_knob(store: &Store, content: &ContentBox) -> (f64, f64) {
    let top = content.point(0.5, 0.0);
    let length =
        40.0 * 0.75 * super::viewport::handle_scale(&store.overlay_editor.viewport.metrics)
            / display_scale(store);
    let (sin, cos) = content.rotation.to_radians().sin_cos();
    (top.0 + length * sin, top.1 - length * cos)
}

/// The press radius of a handle in scene pixels: half the drawn handle plus Moveable's slack.
fn handle_radius(store: &Store) -> f64 {
    let metrics = &store.overlay_editor.viewport.metrics;
    let size = 0.741 * metrics.rem * 0.75 * super::viewport::handle_scale(metrics);
    (size / 2.0 + 3.0) / display_scale(store)
}

fn selection_preferred(
    store: &Store,
    ids: &[String],
    preferred: Option<&str>,
) -> (Vec<String>, Option<String>) {
    let ids = normalize_selection_ids(ids, &store.editor.config.widget_ids());
    let primary = primary_selection_id(&ids, preferred);
    (ids, primary)
}

fn commit_selection(store: &mut Store, ids: &[String], preferred: Option<&str>) {
    let (ids, primary) = selection_preferred(store, ids, preferred);
    store.editor.set_widget_selection(&ids, primary.as_deref());
}

/// A press on the stage at `point` (stage pixels).
pub fn pointer_down(store: &mut Store, point: (f64, f64), modifiers: PointerModifiers) {
    cancel_gesture(store);
    let scene_point = stage_to_scene(store, point);
    let radius = handle_radius(store);
    if let Some((_, handle)) = handles(store).into_iter().find(|(position, _)| {
        (position.0 - scene_point.0).hypot(position.1 - scene_point.1) <= radius
    }) {
        let ids = effective_selection(store);
        let kind = match handle {
            Handle::Resize { direction } => GestureKind::Resize { direction },
            Handle::Rotate => GestureKind::Rotate,
        };
        begin_gesture(store, ids, kind, scene_point);
        return;
    }
    let Some(widget_id) = hit_widget(store, scene_point) else {
        return start_marquee(store, point, scene_point, modifiers);
    };
    let selected = store.editor.selected_widget_ids.clone();
    let is_selected = selected.contains(&widget_id);
    let axis_lock_drag = modifiers.ctrl && is_selected && !modifiers.meta && !modifiers.shift;
    if is_selected && selected.len() > 1 && (axis_lock_drag || !modifiers.selects()) {
        begin_gesture(store, selected, GestureKind::Drag, scene_point);
        return;
    }
    if axis_lock_drag {
        begin_gesture(store, vec![widget_id], GestureKind::Drag, scene_point);
        return;
    }
    if modifiers.selects() {
        let primary = store.editor.selected_widget_id.clone();
        let (ids, preferred) = if is_selected {
            let ids: Vec<String> = selected.into_iter().filter(|id| *id != widget_id).collect();
            let preferred = if primary.as_deref() == Some(widget_id.as_str()) {
                None
            } else {
                primary
            };
            (ids, preferred)
        } else {
            let mut ids = selected;
            ids.push(widget_id.clone());
            (ids, Some(widget_id))
        };
        commit_selection(store, &ids, preferred.as_deref());
        return;
    }
    if selected.len() != 1 || store.editor.selected_widget_id.as_deref() != Some(widget_id.as_str())
    {
        commit_selection(store, &[widget_id.clone()], Some(&widget_id));
    }
    begin_gesture(store, vec![widget_id], GestureKind::Drag, scene_point);
}

fn start_marquee(
    store: &mut Store,
    stage_point: (f64, f64),
    scene_point: (f64, f64),
    modifiers: PointerModifiers,
) {
    let additive = modifiers.selects();
    let base_ids = if additive {
        store.editor.selected_widget_ids.clone()
    } else {
        Vec::new()
    };
    let (width, height) = scene_size(store);
    store.overlay_editor.pointer = PointerState::Marquee(Marquee {
        additive,
        preview_ids: base_ids.clone(),
        base_ids,
        start_scene: (
            scene_point.0.clamp(0.0, width),
            scene_point.1.clamp(0.0, height),
        ),
        start_stage: stage_point,
        has_moved: false,
        stage_rect: Rect {
            left: stage_point.0,
            top: stage_point.1,
            width: 0.0,
            height: 0.0,
        },
    });
    store.editor.set_marquee_selecting(true);
}

fn begin_gesture(store: &mut Store, ids: Vec<String>, kind: GestureKind, start: (f64, f64)) {
    let Some(theme) = store.theme.clone() else {
        return;
    };
    let widgets = preview_widgets(store, &theme);
    let presented = store.preview.presented();
    let mut origins = BTreeMap::new();
    for id in &ids {
        let (Some(widget), Some(shown)) = (
            widgets.iter().find(|widget| &widget.id == id),
            presented.get(id),
        ) else {
            return;
        };
        let scene = store
            .preview
            .request_scene(id)
            .cloned()
            .unwrap_or(Value::Null);
        origins.insert(
            id.clone(),
            Origin {
                kind: widget.kind.clone(),
                data: widget.data.clone(),
                content: ContentBox::of(&shown.geometry),
                scene,
            },
        );
    }
    let group = ids.len() > 1;
    let primary = ids
        .last()
        .and_then(|id| widgets.iter().find(|widget| &widget.id == id));
    let options = resize_options(primary, group);
    if (kind == GestureKind::Drag && !options.can_drag)
        || (kind == GestureKind::Rotate && !options.can_rotate)
    {
        return;
    }
    let start_box = if group {
        let boxes: Vec<ContentBox> = origins.values().map(|origin| origin.content).collect();
        ContentBox {
            rect: group_bounds(&boxes),
            rotation: 0.0,
            origin: (0.0, 0.0),
        }
    } else {
        origins[&ids[0]].content
    };
    let elements: Vec<Rect> = content_boxes(store)
        .into_iter()
        .filter(|(id, _, _)| !ids.contains(id))
        .map(|(_, _, content)| content.bounds())
        .collect();
    let scene = scene_size(store);
    let grid = store
        .editor_shell
        .editor_snap_to_grid
        .then(|| editor_grid_size(scene));
    store.overlay_editor.pointer = PointerState::Gesture(Box::new(Gesture {
        kind,
        ids,
        group,
        keep_ratio: options.keep_ratio,
        start,
        origins,
        start_box,
        lines: guidelines(&elements, scene, grid),
        axis: None,
        last_angle: None,
        turned: 0.0,
        patches: BTreeMap::new(),
        moved: false,
        guides: Vec::new(),
        config: std::rc::Rc::as_ptr(&store.editor.config),
        selection: store.editor.selected_widget_ids.clone(),
    }));
}

/// `dragTranslation`: with Ctrl, nothing until 3 scene px, then only the dominant axis.
fn drag_translation(axis: &mut Option<bool>, delta: (f64, f64), locked: bool) -> (f64, f64) {
    if !locked {
        *axis = None;
        return delta;
    }
    if axis.is_none() {
        if delta.0.abs().max(delta.1.abs()) < 3.0 {
            return (0.0, 0.0);
        }
        *axis = Some(delta.0.abs() >= delta.1.abs());
    }
    if *axis == Some(true) {
        (delta.0, 0.0)
    } else {
        (0.0, delta.1)
    }
}

fn number_of(data: &Map<String, Value>, key: &str) -> f64 {
    data.get(key).and_then(Value::as_f64).unwrap_or_default()
}

/// A resize request for one widget, from its gesture-start data.
fn resize_request(
    origin: &Origin,
    geometry: Map<String, Value>,
    second: f64,
    commit: bool,
) -> Map<String, Value> {
    let mut request = Map::new();
    request.insert("type".into(), Value::String(origin.kind.clone()));
    request.insert("widget".into(), Value::Object(origin.data.clone()));
    request.insert("scene".into(), origin.scene.clone());
    request.insert("second".into(), Value::from(second));
    if matches!(
        origin.kind.as_str(),
        "course" | "elevation" | "diving_profile"
    ) {
        request.insert("commit".into(), Value::Bool(commit));
    }
    request.extend(geometry);
    request
}

/// The size a resize asks for: the pointer delta in the box's own axes, along the handle, the ratio
/// kept on the box's diagonal when it must be; snapped (unrotated boxes) and rounded.
fn resized_size(gesture: &Gesture, direction: (f64, f64), delta: (f64, f64)) -> (f64, f64) {
    let start = gesture.start_box;
    let (sin, cos) = (-start.rotation).to_radians().sin_cos();
    let local = (delta.0 * cos - delta.1 * sin, delta.0 * sin + delta.1 * cos);
    let (w, h) = (start.rect.width, start.rect.height);
    let (mut width, mut height) = if gesture.keep_ratio {
        let scale = if direction.0 != 0.0 && direction.1 != 0.0 {
            let axis = (direction.0 * w, direction.1 * h);
            1.0 + (local.0 * axis.0 + local.1 * axis.1) / (axis.0 * axis.0 + axis.1 * axis.1)
        } else if direction.0 != 0.0 {
            1.0 + direction.0 * local.0 / w
        } else {
            1.0 + direction.1 * local.1 / h
        };
        (w * scale, h * scale)
    } else {
        (w + direction.0 * local.0, h + direction.1 * local.1)
    };
    if start.rotation == 0.0 {
        // The moving edges snap: x for a horizontal handle component, y for a vertical one.
        let fixed_x = if direction.0 > 0.0 {
            start.rect.left
        } else {
            start.rect.right()
        };
        let fixed_y = if direction.1 > 0.0 {
            start.rect.top
        } else {
            start.rect.bottom()
        };
        let moving_x = fixed_x + direction.0 * width;
        let moving_y = fixed_y + direction.1 * height;
        let dx = if direction.0 != 0.0 {
            snap_axis(&[moving_x], &gesture.lines, true)
        } else {
            None
        };
        let dy = if direction.1 != 0.0 {
            snap_axis(&[moving_y], &gesture.lines, false)
        } else {
            None
        };
        let dw = dx.map(|correction| direction.0 * correction);
        let dh = dy.map(|correction| direction.1 * correction);
        if gesture.keep_ratio {
            // The dimension that needs the relatively smaller change wins; the other follows the ratio.
            let relative = |change: Option<f64>, size: f64| {
                change.map(|change| (change / size.max(1e-9)).abs())
            };
            match (relative(dw, width), relative(dh, height)) {
                (Some(rw), Some(rh)) if rw <= rh => {
                    let next = width + dw.unwrap_or_default();
                    height *= next / width.max(1e-9);
                    width = next;
                }
                (Some(_), None) => {
                    let next = width + dw.unwrap_or_default();
                    height *= next / width.max(1e-9);
                    width = next;
                }
                (_, Some(_)) => {
                    let next = height + dh.unwrap_or_default();
                    width *= next / height.max(1e-9);
                    height = next;
                }
                (None, None) => {}
            }
        } else {
            width += dw.unwrap_or_default();
            height += dh.unwrap_or_default();
        }
    }
    let width = width.max(0.0).round();
    let height = if gesture.group && gesture.keep_ratio {
        width * h / w
    } else {
        height.max(0.0).round()
    };
    (width, height)
}

/// Resolves the gesture's patches for a pointer at `point` (scene pixels).
fn resolve(
    store: &mut Store,
    gesture: &mut Gesture,
    point: (f64, f64),
    modifiers: PointerModifiers,
    commit: bool,
) -> Result<(), String> {
    let delta = (point.0 - gesture.start.0, point.1 - gesture.start.1);
    let second = store.editor.selected_second;
    let scene = scene_size(store);
    gesture.patches.clear();
    match gesture.kind.clone() {
        GestureKind::Drag => {
            let start = gesture.start_box.bounds();
            let moved = start.translated(delta.0, delta.1);
            let mut translate = delta;
            if delta.0 != 0.0 {
                translate.0 += snap_axis(
                    &[moved.left, moved.right(), moved.center_x()],
                    &gesture.lines,
                    true,
                )
                .unwrap_or_default();
            }
            if delta.1 != 0.0 {
                translate.1 += snap_axis(
                    &[moved.top, moved.bottom(), moved.center_y()],
                    &gesture.lines,
                    false,
                )
                .unwrap_or_default();
            }
            let translate = drag_translation(&mut gesture.axis, translate, modifiers.ctrl);
            let target = start.translated(translate.0, translate.1);
            gesture.guides = guides(
                &target,
                &[target.left, target.right(), target.center_x()],
                &[target.top, target.bottom(), target.center_y()],
                &gesture.lines,
                scene,
            );
            for (id, origin) in &gesture.origins {
                let mut patch = Map::new();
                patch.insert(
                    "x".into(),
                    number(number_of(&origin.data, "x") + translate.0),
                );
                patch.insert(
                    "y".into(),
                    number(number_of(&origin.data, "y") + translate.1),
                );
                gesture.patches.insert(id.clone(), patch);
            }
        }
        GestureKind::Rotate => {
            let id = gesture.ids[0].clone();
            let origin = &gesture.origins[&id];
            let pivot = (
                origin.content.rect.left + origin.content.origin.0,
                origin.content.rect.top + origin.content.origin.1,
            );
            let angle = (point.1 - pivot.1).atan2(point.0 - pivot.0).to_degrees();
            let start_angle = (gesture.start.1 - pivot.1)
                .atan2(gesture.start.0 - pivot.0)
                .to_degrees();
            let previous = gesture.last_angle.unwrap_or(start_angle);
            let mut step = angle - previous;
            if step > 180.0 {
                step -= 360.0;
            } else if step < -180.0 {
                step += 360.0;
            }
            gesture.turned += step;
            gesture.last_angle = Some(angle);
            let rotation = number_of(&origin.data, "rotation") + gesture.turned;
            gesture.patches.insert(
                id,
                rotation_patch(&origin.kind, &origin.data, rotation, &origin.content),
            );
            gesture.guides.clear();
        }
        GestureKind::Resize { direction } => {
            let (width, height) = resized_size(gesture, direction, delta);
            let start = gesture.start_box;
            if gesture.group {
                let factor = width / start.rect.width.max(1e-9);
                let (_, group_point) = resize_anchor(&start.rect, direction, 0.0, (0.0, 0.0));
                for (id, origin) in &gesture.origins {
                    let content = origin.content;
                    let rotation = if rotates_anchor(&origin.kind) {
                        number_of(&origin.data, "rotation")
                    } else {
                        0.0
                    };
                    let (fixed, anchor) =
                        resize_anchor(&content.rect, direction, rotation, content.origin);
                    let mut geometry = resize_geometry(
                        &origin.kind,
                        &origin.data,
                        &content.rect,
                        content.rect.width * factor,
                        content.rect.height * factor,
                    );
                    geometry.insert("fixed".into(), serde_json::json!(fixed));
                    geometry.insert(
                        "point".into(),
                        serde_json::json!([
                            group_point.0 + (anchor.0 - group_point.0) * factor,
                            group_point.1 + (anchor.1 - group_point.1) * factor
                        ]),
                    );
                    let patch = store
                        .preview
                        .resize_widget(id, resize_request(origin, geometry, second, commit))?;
                    gesture.patches.insert(id.clone(), patch);
                }
            } else {
                let id = gesture.ids[0].clone();
                let origin = &gesture.origins[&id];
                let rotation = if rotates_anchor(&origin.kind) {
                    number_of(&origin.data, "rotation")
                } else {
                    0.0
                };
                let (fixed, anchor) = resize_anchor(&start.rect, direction, rotation, start.origin);
                let mut geometry =
                    resize_geometry(&origin.kind, &origin.data, &start.rect, width, height);
                geometry.insert("fixed".into(), serde_json::json!(fixed));
                geometry.insert("point".into(), serde_json::json!([anchor.0, anchor.1]));
                let patch = store
                    .preview
                    .resize_widget(&id, resize_request(origin, geometry, second, commit))?;
                gesture.patches.insert(id, patch);
            }
            gesture.guides = if start.rotation == 0.0 {
                let fixed_x = if direction.0 > 0.0 {
                    start.rect.left
                } else {
                    start.rect.right()
                };
                let fixed_y = if direction.1 > 0.0 {
                    start.rect.top
                } else {
                    start.rect.bottom()
                };
                let target = Rect::bounding(&[
                    (fixed_x, fixed_y),
                    (
                        fixed_x + direction.0.signum() * width * direction.0.abs().max(0.0),
                        fixed_y + direction.1.signum() * height * direction.1.abs(),
                    ),
                ]);
                let xs: Vec<f64> = if direction.0 != 0.0 {
                    vec![fixed_x + direction.0 * width]
                } else {
                    Vec::new()
                };
                let ys: Vec<f64> = if direction.1 != 0.0 {
                    vec![fixed_y + direction.1 * height]
                } else {
                    Vec::new()
                };
                guides(&target, &xs, &ys, &gesture.lines, scene)
            } else {
                Vec::new()
            };
        }
    }
    Ok(())
}

/// A pointer move: a running gesture resolves; a marquee follows; otherwise hover.
pub fn pointer_move(store: &mut Store, point: (f64, f64), modifiers: PointerModifiers) {
    let scene_point = stage_to_scene(store, point);
    match std::mem::take(&mut store.overlay_editor.pointer) {
        PointerState::Gesture(mut gesture) => {
            gesture.moved = true;
            match resolve(store, &mut gesture, scene_point, modifiers, false) {
                Ok(()) => {
                    store.drafts.set_gesture_patches(gesture.patches.clone());
                    store.overlay_editor.pointer = PointerState::Gesture(gesture);
                }
                Err(error) => {
                    store.preview.error = Some(error);
                    store.drafts.clear_gesture_patches();
                }
            }
        }
        PointerState::Marquee(mut marquee) => {
            let (width, height) = scene_size(store);
            let clamped = (
                scene_point.0.clamp(0.0, width),
                scene_point.1.clamp(0.0, height),
            );
            let scene_rect = Rect::bounding(&[marquee.start_scene, clamped]);
            marquee.stage_rect = Rect::bounding(&[marquee.start_stage, point]);
            marquee.has_moved = marquee.stage_rect.width > 2.0 || marquee.stage_rect.height > 2.0;
            let hits: Vec<String> = if marquee.has_moved {
                content_boxes(store)
                    .into_iter()
                    .filter(|(_, _, content)| content.bounds().intersects(&scene_rect))
                    .map(|(id, _, _)| id)
                    .collect()
            } else {
                Vec::new()
            };
            marquee.preview_ids = if marquee.additive {
                let mut ids = marquee.base_ids.clone();
                ids.extend(hits);
                normalize_selection_ids(&ids, &store.editor.config.widget_ids())
            } else {
                hits
            };
            if marquee.has_moved {
                let ids = marquee.preview_ids.clone();
                commit_selection(store, &ids, None);
            }
            store.overlay_editor.pointer = PointerState::Marquee(marquee);
        }
        PointerState::Idle => {
            store.overlay_editor.hovered_widget_id = hit_widget(store, scene_point);
        }
    }
}

/// The release: a gesture commits its patches as one action (one undo step); a marquee keeps its
/// preview, or restores the base selection when it did not move.
pub fn pointer_up(store: &mut Store, point: (f64, f64), modifiers: PointerModifiers) {
    match std::mem::take(&mut store.overlay_editor.pointer) {
        PointerState::Gesture(mut gesture) => {
            store.drafts.clear_gesture_patches();
            if !gesture.moved {
                return;
            }
            let scene_point = stage_to_scene(store, point);
            // Profiles settle their resize with `commit`, as the flush on mouseup does.
            if let Err(error) = resolve(store, &mut gesture, scene_point, modifiers, true) {
                store.preview.error = Some(error);
                return;
            }
            let updates: Vec<(String, Map<String, Value>)> = gesture.patches.into_iter().collect();
            if let Err(error) = store
                .editor
                .config
                .as_ref()
                .clone()
                .update_widgets(&updates)
                .map(|()| ())
            {
                store.preview.error = Some(error);
                return;
            }
            store
                .editor
                .edit_config(|config| config.update_widgets(&updates).expect("checked above"));
        }
        PointerState::Marquee(marquee) => {
            store.editor.set_marquee_selecting(false);
            let primary = store.editor.selected_widget_id.clone();
            let ids = if marquee.has_moved {
                marquee.preview_ids
            } else {
                marquee.base_ids
            };
            commit_selection(store, &ids, primary.as_deref());
        }
        PointerState::Idle => {}
    }
}

/// The pointer left the canvas: hover ends.
pub fn pointer_leave(store: &mut Store) {
    store.overlay_editor.hovered_widget_id = None;
}

/// [`pointer_leave`] as an action.
pub fn pointer_leave_action(store: &mut Store, _: &mut crate::store::services::Services) {
    pointer_leave(store);
}

/// The single selection's content box, or the group's box.
fn selection_box(store: &Store) -> Option<ContentBox> {
    let ids = effective_selection(store);
    let presented = store.preview.presented();
    let boxes: Vec<ContentBox> = ids.iter().filter_map(|id| presented.get(id).map(|widget| ContentBox::of(&widget.geometry))).collect();
    match boxes.len() {
        0 => None,
        1 => Some(boxes[0]),
        _ => Some(ContentBox { rect: group_bounds(&boxes), rotation: 0.0, origin: (0.0, 0.0) }),
    }
}

/// The rotation the handles show (degrees).
pub fn selection_rotation(store: &Store) -> f64 {
    selection_box(store).map_or(0.0, |content| content.rotation)
}

/// The middle of the selection's top edge, where the rotation line starts.
pub fn selection_top_middle(store: &Store) -> Option<(f64, f64)> {
    selection_box(store).map(|content| content.point(0.5, 0.0))
}

/// Cancels a running gesture: Escape, focus loss, a selection change, a config replacement.
/// Returns whether one ran.
pub fn cancel_gesture(store: &mut Store) -> bool {
    match &store.overlay_editor.pointer {
        PointerState::Gesture(_) => {
            store.overlay_editor.pointer = PointerState::Idle;
            store.drafts.clear_gesture_patches();
            true
        }
        PointerState::Marquee(_) => {
            store.overlay_editor.pointer = PointerState::Idle;
            store.editor.set_marquee_selecting(false);
            true
        }
        PointerState::Idle => false,
    }
}

/// Reconcile: a running transform is cancelled when the document or the selection it started on
/// changed (`useEffect(cancel, [config, selectionIds])`).
pub fn reconcile_gesture(store: &mut Store) {
    let stale = match &store.overlay_editor.pointer {
        PointerState::Gesture(gesture) => gesture.config != std::rc::Rc::as_ptr(&store.editor.config) || gesture.selection != store.editor.selected_widget_ids,
        _ => false,
    };
    if stale {
        cancel_gesture(store);
    }
}

/// A widget's type badge: above its content's top-left corner, rotated with it, in stage pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub id: String,
    pub kind: String,
    pub left: f64,
    pub top: f64,
    pub rotation: f64,
}

/// `buildWidgetBadges`: hovered and selected widgets, hidden during group gestures.
pub fn badges(store: &Store) -> Vec<Badge> {
    if matches!(&store.overlay_editor.pointer, PointerState::Gesture(gesture) if gesture.group) {
        return Vec::new();
    }
    let mut visible = effective_selection(store);
    if let Some(hovered) = &store.overlay_editor.hovered_widget_id {
        visible.push(hovered.clone());
    }
    let scale = display_scale(store);
    let layout = stage(store);
    content_boxes(store)
        .into_iter()
        .filter(|(id, _, _)| visible.contains(id))
        .map(|(id, kind, content)| {
            let (sin, cos) = content.rotation.to_radians().sin_cos();
            let (ox, oy) = content.origin;
            let left = content.rect.left + ox - ox * cos + oy * sin;
            let top = content.rect.top + oy - ox * sin - oy * cos;
            Badge {
                id,
                kind,
                left: layout.scene_left + left * scale + 2.0 * cos + 20.0 * sin,
                top: layout.scene_top + (top * scale + 2.0 * sin - 20.0 * cos).max(0.0),
                rotation: sin.atan2(cos).to_degrees(),
            }
        })
        .collect()
}

/// The selection outlines in stage pixels: each selected widget's rotated content box, and the
/// group's box (children hidden while a group drags).
pub fn selection_outlines(store: &Store) -> Vec<[(f64, f64); 4]> {
    let ids = effective_selection(store);
    let presented = store.preview.presented();
    let boxes: Vec<ContentBox> = ids
        .iter()
        .filter_map(|id| {
            presented
                .get(id)
                .map(|widget| ContentBox::of(&widget.geometry))
        })
        .collect();
    let mut outlines = Vec::new();
    let group_drag = matches!(&store.overlay_editor.pointer, PointerState::Gesture(gesture) if gesture.group && gesture.kind == GestureKind::Drag);
    if boxes.len() > 1 {
        let group = ContentBox {
            rect: group_bounds(&boxes),
            rotation: 0.0,
            origin: (0.0, 0.0),
        };
        outlines.push(group.corners().map(|corner| scene_to_stage(store, corner)));
    }
    if !group_drag {
        for content in &boxes {
            outlines.push(
                content
                    .corners()
                    .map(|corner| scene_to_stage(store, corner)),
            );
        }
    }
    outlines
}

/// The running gesture's guides.
pub fn current_guides(store: &Store) -> &[Guide] {
    match &store.overlay_editor.pointer {
        PointerState::Gesture(gesture) => &gesture.guides,
        _ => &[],
    }
}

/// The marquee's rectangle in stage pixels, while one runs.
pub fn marquee_rect(store: &Store) -> Option<Rect> {
    match &store.overlay_editor.pointer {
        PointerState::Marquee(marquee) => Some(marquee.stage_rect),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::overlay_editor::viewport::ViewportMetrics;
    use crate::features::widget_preview::preview_frame;
    use crate::lib::template::ThemeColors;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;
    use ovrley_widgets::model::document::OverlayConfig;
    use serde_json::json;

    fn backdrop(id: &str, x: f64, y: f64) -> Value {
        let mut data = ovrley_widgets::model::document::create_widget_defaults(
            &ovrley_widgets::model::document::WidgetSelection {
                kind: "backdrop".into(),
                ..Default::default()
            },
            &ovrley_core::normalize::template::template_manifest().globals,
            &Default::default(),
        )
        .unwrap();
        data.insert("id".into(), json!(id));
        data.insert("x".into(), json!(x));
        data.insert("y".into(), json!(y));
        Value::Object(data)
    }

    /// A 1000×500 scene shown at scale 1 with its top-left corner at the stage's (16, 16).
    fn editor(widgets: Vec<Value>) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let mut store = Store::default();
        store.theme = Some(ThemeColors {
            ice: "#86aaaa".into(),
            aqua: "#40e0d0".into(),
        });
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(move |store, _| {
            store
                .history
                .set_mode(crate::features::undo_redo::HistoryMode::Reset);
            store.editor.set_config(
                OverlayConfig::from_value(
                    &json!({"scene": {"width": 1000, "height": 500}, "backdrops": widgets}),
                )
                .unwrap(),
            );
            store.editor.set_selected_widget_id(None);
            super::super::set_viewport_metrics(
                store,
                ViewportMetrics {
                    editor_width: 1104.0,
                    editor_height: 604.0,
                    scroll_width: 1104.0,
                    scroll_height: 532.0,
                    padding: 16.0,
                    scale_factor: 2.0,
                    rem: 16.0,
                },
            );
        });
        for _ in 0..50 {
            fakes.ui_post.drain(&dispatcher);
            dispatcher.dispatch(preview_frame);
        }
        (dispatcher, fakes)
    }

    fn at(store: &Store, scene: (f64, f64)) -> (f64, f64) {
        scene_to_stage(store, scene)
    }

    fn press(dispatcher: &Dispatcher, scene: (f64, f64), modifiers: PointerModifiers) {
        dispatcher.dispatch(move |store, _| {
            let point = at(store, scene);
            pointer_down(store, point, modifiers);
        });
    }

    fn drag_to(dispatcher: &Dispatcher, scene: (f64, f64), modifiers: PointerModifiers) {
        dispatcher.dispatch(move |store, _| {
            let point = at(store, scene);
            pointer_move(store, point, modifiers);
        });
    }

    fn release(dispatcher: &Dispatcher, scene: (f64, f64)) {
        dispatcher.dispatch(move |store, _| {
            let point = at(store, scene);
            pointer_up(store, point, PointerModifiers::default());
        });
    }

    fn selection(dispatcher: &Dispatcher) -> Vec<String> {
        dispatcher.with_store(|store| store.editor.selected_widget_ids.clone())
    }

    fn position(dispatcher: &Dispatcher, id: &str) -> (f64, f64) {
        dispatcher.with_store(|store| {
            let widget = store.editor.config.widget(id).unwrap();
            (widget["x"].as_f64().unwrap(), widget["y"].as_f64().unwrap())
        })
    }

    #[test]
    fn a_press_selects_the_topmost_widget_and_modifiers_toggle() {
        let (dispatcher, _) = editor(vec![
            backdrop("a", 100.0, 100.0),
            backdrop("b", 150.0, 120.0),
        ]);
        press(&dispatcher, (160.0, 130.0), PointerModifiers::default());
        release(&dispatcher, (160.0, 130.0));
        assert_eq!(selection(&dispatcher), ["b"], "the later widget is on top");
        let shift = PointerModifiers {
            shift: true,
            ..Default::default()
        };
        press(&dispatcher, (110.0, 105.0), shift);
        release(&dispatcher, (110.0, 105.0));
        assert_eq!(selection(&dispatcher), ["a", "b"]);
        press(&dispatcher, (160.0, 130.0), shift);
        release(&dispatcher, (160.0, 130.0));
        assert_eq!(selection(&dispatcher), ["a"]);
    }

    #[test]
    fn a_marquee_selects_live_and_a_click_on_empty_stage_clears() {
        let (dispatcher, _) = editor(vec![
            backdrop("a", 100.0, 100.0),
            backdrop("b", 600.0, 300.0),
        ]);
        press(&dispatcher, (50.0, 50.0), PointerModifiers::default());
        drag_to(&dispatcher, (120.0, 120.0), PointerModifiers::default());
        assert_eq!(
            selection(&dispatcher),
            ["a"],
            "the selection follows the marquee"
        );
        assert!(dispatcher.with_store(|store| store.editor.is_marquee_selecting));
        release(&dispatcher, (120.0, 120.0));
        assert!(!dispatcher.with_store(|store| store.editor.is_marquee_selecting));
        press(&dispatcher, (900.0, 50.0), PointerModifiers::default());
        release(&dispatcher, (900.0, 50.0));
        assert!(
            selection(&dispatcher).is_empty(),
            "a release without a move restores the empty base"
        );
    }

    #[test]
    fn a_drag_previews_through_gesture_patches_and_commits_one_step() {
        let (dispatcher, _) = editor(vec![backdrop("a", 100.0, 100.0)]);
        let before = dispatcher.with_store(|store| store.history.past.len());
        press(&dispatcher, (110.0, 110.0), PointerModifiers::default());
        drag_to(&dispatcher, (140.0, 150.0), PointerModifiers::default());
        assert_eq!(
            position(&dispatcher, "a"),
            (100.0, 100.0),
            "the document waits for the release"
        );
        assert!(dispatcher.with_store(|store| store.drafts.gesture_patch("a").is_some()));
        drag_to(&dispatcher, (170.0, 133.0), PointerModifiers::default());
        release(&dispatcher, (170.0, 133.0));
        assert_eq!(position(&dispatcher, "a"), (160.0, 123.0));
        assert_eq!(
            dispatcher.with_store(|store| store.history.past.len()),
            before + 1,
            "selecting is not an undo step; the drag is one"
        );
        assert!(dispatcher.with_store(|store| store.drafts.gesture_patch("a").is_none()));
    }

    #[test]
    fn drags_snap_to_scene_lines_within_five_pixels() {
        let (dispatcher, _) = editor(vec![backdrop("a", 100.0, 100.0)]);
        press(&dispatcher, (110.0, 110.0), PointerModifiers::default());
        drag_to(&dispatcher, (13.0, 110.0), PointerModifiers::default());
        release(&dispatcher, (13.0, 110.0));
        assert_eq!(
            position(&dispatcher, "a"),
            (0.0, 100.0),
            "the left edge snapped to the scene's left"
        );
    }

    #[test]
    fn ctrl_locks_the_dominant_axis_after_three_pixels() {
        let mut axis = None;
        assert_eq!(drag_translation(&mut axis, (2.0, 1.0), true), (0.0, 0.0));
        assert_eq!(drag_translation(&mut axis, (10.0, 4.0), true), (10.0, 0.0));
        assert_eq!(
            drag_translation(&mut axis, (10.0, 40.0), true),
            (10.0, 0.0),
            "the axis stays"
        );
        assert_eq!(
            drag_translation(&mut axis, (10.0, 40.0), false),
            (10.0, 40.0)
        );
    }

    #[test]
    fn escape_cancels_a_gesture_without_changes() {
        let (dispatcher, _) = editor(vec![backdrop("a", 100.0, 100.0)]);
        press(&dispatcher, (110.0, 110.0), PointerModifiers::default());
        drag_to(&dispatcher, (200.0, 200.0), PointerModifiers::default());
        assert!(dispatcher
            .dispatch_with_result(|store, _| cancel_gesture(store))
            .unwrap());
        release(&dispatcher, (200.0, 200.0));
        assert_eq!(position(&dispatcher, "a"), (100.0, 100.0));
    }

    #[test]
    fn corner_handles_resize_through_the_rust_policy() {
        let (dispatcher, _) = editor(vec![backdrop("a", 100.0, 100.0)]);
        press(&dispatcher, (110.0, 110.0), PointerModifiers::default());
        release(&dispatcher, (110.0, 110.0));
        let corner = dispatcher.with_store(|store| {
            handles(store)
                .into_iter()
                .find(|(_, handle)| {
                    *handle
                        == Handle::Resize {
                            direction: (1.0, 1.0),
                        }
                })
                .map(|(point, _)| point)
                .unwrap()
        });
        press(&dispatcher, corner, PointerModifiers::default());
        drag_to(
            &dispatcher,
            (corner.0 + 50.0, corner.1 + 20.0),
            PointerModifiers::default(),
        );
        release(&dispatcher, (corner.0 + 50.0, corner.1 + 20.0));
        dispatcher.with_store(|store| {
            let widget = store.editor.config.widget("a").unwrap();
            let variant = &widget["display_variants"]["rectangle"];
            assert!(variant["width"].as_f64().unwrap() > 200.0, "{widget:?}");
            assert_eq!(widget["x"], json!(100));
        });
    }

    #[test]
    fn badges_show_for_selected_and_hovered_widgets() {
        let (dispatcher, _) = editor(vec![
            backdrop("a", 100.0, 100.0),
            backdrop("b", 600.0, 300.0),
        ]);
        press(&dispatcher, (110.0, 110.0), PointerModifiers::default());
        release(&dispatcher, (110.0, 110.0));
        dispatcher.dispatch(|store, _| {
            let point = at(store, (610.0, 310.0));
            pointer_move(store, point, PointerModifiers::default());
        });
        let ids: Vec<String> = dispatcher
            .with_store(|store| badges(store).into_iter().map(|badge| badge.id).collect());
        assert_eq!(ids, ["a", "b"]);
    }
}

/// How an agent sizes a widget: a width and height, or a factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizeTarget {
    Size { width: f64, height: f64 },
    Factor(f64),
}

fn widget_origin(store: &Store, widget_id: &str) -> Result<Origin, String> {
    store.editor.config.widget(widget_id)?;
    let theme = store.theme.clone().ok_or("The editor has not started yet.")?;
    let widget = preview_widgets(store, &theme).into_iter().find(|widget| widget.id == widget_id).ok_or_else(|| format!("Unknown widget ID: {widget_id}"))?;
    let shown = store.preview.presented().get(widget_id).ok_or_else(|| format!("{widget_id} has not rendered yet; try again in a moment."))?;
    let scene = store.preview.request_scene(widget_id).cloned().unwrap_or(Value::Null);
    Ok(Origin { kind: widget.kind, data: widget.data, content: ContentBox::of(&shown.geometry), scene })
}

/// The transform's resize for an agent (`widget_resize`): the Rust resize policy, anchored at the
/// widget's top-left corner. One action, so one undo step.
pub fn resize_widget(store: &mut Store, widget_id: &str, target: ResizeTarget) -> Result<(), String> {
    let origin = widget_origin(store, widget_id)?;
    let rect = origin.content.rect;
    let (width, height) = match target {
        ResizeTarget::Size { width, height } => (width, height),
        ResizeTarget::Factor(factor) => (rect.width * factor, rect.height * factor),
    };
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err("The size must be positive.".into());
    }
    let rotation = if rotates_anchor(&origin.kind) { number_of(&origin.data, "rotation") } else { 0.0 };
    let (fixed, anchor) = resize_anchor(&rect, (1.0, 1.0), rotation, origin.content.origin);
    let mut geometry = resize_geometry(&origin.kind, &origin.data, &rect, width, height);
    geometry.insert("fixed".into(), serde_json::json!(fixed));
    geometry.insert("point".into(), serde_json::json!([anchor.0, anchor.1]));
    let second = store.editor.selected_second;
    let patch = store.preview.resize_widget(widget_id, resize_request(&origin, geometry, second, true))?;
    store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), patch)]))
}

/// The transform's rotation for an agent (`widget_rotate`): course and raster only, about their
/// content centre, in whole degrees.
pub fn rotate_widget(store: &mut Store, widget_id: &str, rotation: f64) -> Result<(), String> {
    let origin = widget_origin(store, widget_id)?;
    if origin.kind != "course" && origin.kind != "raster" {
        return Err(format!("Only course and image widgets rotate; {widget_id} is a {} widget.", origin.kind));
    }
    let patch = rotation_patch(&origin.kind, &origin.data, rotation, &origin.content);
    store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), patch)]))
}

/// A widget's presented content rectangle, in scene pixels.
pub fn content_rect(store: &Store, widget_id: &str) -> Option<Rect> {
    store.preview.presented().get(widget_id).map(|widget| ContentBox::of(&widget.geometry).rect)
}
