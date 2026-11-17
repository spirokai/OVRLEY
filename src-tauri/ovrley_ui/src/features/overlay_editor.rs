//! The overlay-editor feature: its projection into the `OverlayEditor` global, the widget images
//! (D10), the preview frame's scheduling (D9) and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::platform::key_input::{key_input, SlintModifiers};
use crate::{AppWindow, BackgroundMode, BadgeView, EditorCursor, GuideLabelView, HandleView, OverlayEditor, Theme, WidgetView};
use ovrley_app::features::overlay_editor::geometry::GuideStyle;
use ovrley_app::features::overlay_editor::interaction::{self, Handle, PointerModifiers};
use ovrley_app::features::app_shell::EditorBackgroundMode;
use ovrley_app::features::overlay_editor::{self as editor, viewport::ViewportMetrics, WheelInput};
use ovrley_app::features::widget_preview::{present_pending_frame, preview_frame, widget_layer};
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

fn to_slint(mode: EditorBackgroundMode) -> BackgroundMode {
    match mode {
        EditorBackgroundMode::Checker => BackgroundMode::Checker,
        EditorBackgroundMode::Black => BackgroundMode::Black,
        EditorBackgroundMode::White => BackgroundMode::White,
    }
}

fn from_slint(mode: BackgroundMode) -> EditorBackgroundMode {
    match mode {
        BackgroundMode::Checker => EditorBackgroundMode::Checker,
        BackgroundMode::Black => EditorBackgroundMode::Black,
        BackgroundMode::White => EditorBackgroundMode::White,
    }
}

fn color(hex: &str) -> slint::Color {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).expect("a #rrggbb colour");
    slint::Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// The projection's memory: each widget's image with the pixels revision it was made from, so an
/// image is replaced only when its revision changes (parent §4), and whether a frame is already
/// scheduled.
pub struct Projection {
    images: RefCell<HashMap<String, (u32, u32, Image)>>,
    frame_scheduled: std::rc::Rc<Cell<bool>>,
    dispatch: DispatchHandle,
}

impl Projection {
    pub fn new(dispatch: DispatchHandle) -> Self {
        Self {
            images: RefCell::new(HashMap::new()),
            frame_scheduled: std::rc::Rc::new(Cell::new(false)),
            dispatch,
        }
    }

    /// D9: one preview frame for the next event-loop turn, however many actions marked the host.
    fn schedule_frame(&self, store: &Store) {
        if !store.preview.is_dirty() || self.frame_scheduled.replace(true) {
            return;
        }
        let scheduled = std::rc::Rc::clone(&self.frame_scheduled);
        let dispatch = self.dispatch.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            scheduled.set(false);
            dispatch.dispatch(preview_frame);
        });
    }

    pub fn project(&self, window: &AppWindow, store: &Store) {
        let global = window.global::<OverlayEditor>();
        let (scene_width, scene_height) = editor::scene_size(store);
        let display = editor::display_scale(store);
        let stage = editor::stage(store);
        let px = |value: f64| value as f32;
        set_if_changed(global.get_stage_width(), px(stage.content_width), |value| global.set_stage_width(value));
        set_if_changed(global.get_stage_height(), px(stage.content_height), |value| global.set_stage_height(value));
        set_if_changed(global.get_scene_left(), px(stage.scene_left), |value| global.set_scene_left(value));
        set_if_changed(global.get_scene_top(), px(stage.scene_top), |value| global.set_scene_top(value));
        set_if_changed(global.get_scene_width(), px(stage.scene_width), |value| global.set_scene_width(value));
        set_if_changed(global.get_scene_height(), px(stage.scene_height), |value| global.set_scene_height(value));
        let viewport = &store.overlay_editor.viewport;
        set_if_changed(global.get_scroll_x(), px(viewport.scroll_x), |value| global.set_scroll_x(value));
        set_if_changed(global.get_scroll_y(), px(viewport.scroll_y), |value| global.set_scroll_y(value));
        set_if_changed(global.get_display_scale(), px(display), |value| global.set_display_scale(value));
        let label = format!("{} × {}", scene_width, scene_height);
        set_if_changed(global.get_scene_size_label(), SharedString::from(label), |value| global.set_scene_size_label(value));

        let shell = &store.editor_shell;
        set_if_changed(global.get_background_mode(), to_slint(shell.editor_background_mode), |value| global.set_background_mode(value));
        set_if_changed(global.get_background_color(), color(editor::canvas_background_color(shell.editor_background_mode)), |value| global.set_background_color(value));
        set_if_changed(global.get_grid_visible(), shell.editor_grid_visible, |value| global.set_grid_visible(value));
        if shell.editor_grid_visible {
            let commands = editor::grid_commands((scene_width, scene_height), display);
            set_if_changed(global.get_grid_commands(), SharedString::from(commands), |value| global.set_grid_commands(value));
        } else if shell.editor_background_mode == EditorBackgroundMode::Checker && viewport.metrics.rem > 0.0 {
            // The tiles are in rem, which the viewport reports after its first layout.
            let tile = f64::from(window.global::<Theme>().get_checkerboard_tile()) * viewport.metrics.rem * display;
            let columns = (stage.scene_width / tile).ceil() as u32;
            let rows = (stage.scene_height / tile).ceil() as u32;
            let commands = editor::checkerboard_commands(columns, rows);
            set_if_changed(global.get_checkerboard_commands(), SharedString::from(commands), |value| global.set_checkerboard_commands(value));
        }
        set_if_changed(global.get_snap_to_grid(), shell.editor_snap_to_grid, |value| global.set_snap_to_grid(value));
        let zoom = format!("{}%", (shell.editor_zoom_level * 100.0).round());
        set_if_changed(global.get_zoom_label(), SharedString::from(zoom), |value| global.set_zoom_label(value));
        let error = store.preview.error.clone().unwrap_or_default();
        set_if_changed(global.get_error(), SharedString::from(error), |value| global.set_error(value));

        self.project_widgets(&global, store, display);
        project_selection(&global, store);
        self.schedule_frame(store);
    }

    /// The widget images in stacking order (D10): layer, then document order.
    fn project_widgets(&self, global: &OverlayEditor<'_>, store: &Store, display: f64) {
        let presented = store.preview.presented();
        let mut images = self.images.borrow_mut();
        images.retain(|id, _| presented.contains_key(id));
        let mut views: Vec<(u8, usize, WidgetView)> = Vec::new();
        for (order, id) in store.editor.config.widget_ids().iter().enumerate() {
            let Some(widget) = presented.get(id) else { continue };
            // Without activity at the playhead only rasters show; the others stay editable.
            if !store.preview.activity_coverage && widget.kind != "raster" {
                continue;
            }
            let geometry = &widget.geometry.widget;
            let fresh = images.get(id).is_some_and(|(revision, handle, _)| *revision == widget.pixels_revision && *handle == widget.handle);
            if !fresh {
                let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(store.preview.pixels(widget.handle), geometry.width, geometry.height);
                images.insert(id.clone(), (widget.pixels_revision, widget.handle, Image::from_rgba8(buffer)));
            }
            let scale = f64::from(geometry.preview_raster_scale);
            views.push((
                widget_layer(&widget.kind),
                order,
                WidgetView {
                    id: id.into(),
                    image: images[id].2.clone(),
                    x: (f64::from(geometry.left) * display) as f32,
                    y: (f64::from(geometry.top) * display) as f32,
                    width: (f64::from(geometry.width) / scale * display) as f32,
                    height: (f64::from(geometry.height) / scale * display) as f32,
                },
            ));
        }
        views.sort_by_key(|(layer, order, _)| (*layer, *order));
        let next: Vec<WidgetView> = views.into_iter().map(|(_, _, view)| view).collect();
        replace_model(global.get_widgets(), next, |model| global.set_widgets(model));
    }
}

/// Updates a repeater's model. Rows change in place, so a drag moves the existing items instead
/// of recreating them (and their textures) every frame; a changed row count resets the model.
fn replace_model<T: Clone + PartialEq + 'static>(current: ModelRc<T>, next: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() else {
        set(ModelRc::new(VecModel::from(next)));
        return;
    };
    if model.row_count() != next.len() {
        model.set_vec(next);
        return;
    }
    for (row, value) in next.into_iter().enumerate() {
        if model.row_data(row).as_ref() != Some(&value) {
            model.set_row_data(row, value);
        }
    }
}

fn path(segments: impl IntoIterator<Item = ((f64, f64), (f64, f64))>) -> String {
    segments.into_iter().map(|(from, to)| format!("M {} {} L {} {} ", from.0, from.1, to.0, to.1)).collect::<String>().trim_end().to_string()
}

/// The selection layer (D12, D13): outlines, handles, the knob line, guides, labels, the marquee
/// and the badges, in stage pixels.
fn project_selection(global: &OverlayEditor<'_>, store: &Store) {
    let metrics = &store.overlay_editor.viewport.metrics;
    let handle_scale = editor::viewport::handle_scale(metrics);
    let moveable = 0.75 * handle_scale;
    // The map in view mode draws its outline dashed (phase 5 D10).
    let dashed = ovrley_app::features::maps::map_view_widget(store).is_some();
    let corners = interaction::selection_outlines(store);
    let outlines: String = corners
        .iter()
        .filter(|_| !dashed)
        .map(|corners| format!("M {} {} L {} {} L {} {} L {} {} Z ", corners[0].0, corners[0].1, corners[1].0, corners[1].1, corners[2].0, corners[2].1, corners[3].0, corners[3].1))
        .collect();
    set_if_changed(global.get_selection_commands(), SharedString::from(outlines.trim_end()), |value| global.set_selection_commands(value));
    let dashes = if dashed { corners.iter().map(|corners| dashed_outline(corners, 3.0)).collect::<String>() } else { String::new() };
    set_if_changed(global.get_selection_dashed_commands(), SharedString::from(dashes.trim_end()), |value| global.set_selection_dashed_commands(value));
    let device_pixel = (1.0 / metrics.scale_factor.max(1.0)) as f32;
    set_if_changed(global.get_border_width(), device_pixel, |value| global.set_border_width(value));
    set_if_changed(global.get_guide_scale(), moveable as f32, |value| global.set_guide_scale(value));
    let rem = metrics.rem as f32;
    set_if_changed(global.get_handle_size(), 0.741 * rem * moveable as f32, |value| global.set_handle_size(value));
    set_if_changed(global.get_knob_size(), 1.037 * rem * moveable as f32, |value| global.set_knob_size(value));

    let handles = interaction::handles(store);
    let rotation = interaction::selection_rotation(store);
    let mut knob_line = String::new();
    let views: Vec<HandleView> = handles
        .iter()
        .map(|(point, handle)| {
            let stage = interaction::scene_to_stage(store, *point);
            if *handle == Handle::Rotate {
                if let Some(top) = interaction::selection_top_middle(store) {
                    let top = interaction::scene_to_stage(store, top);
                    knob_line = path([(top, stage)]);
                }
            }
            HandleView { x: stage.0 as f32, y: stage.1 as f32, rotation: rotation as f32, knob: *handle == Handle::Rotate }
        })
        .collect();
    replace_model(global.get_handles(), views, |model| global.set_handles(model));
    set_if_changed(global.get_knob_line_commands(), SharedString::from(knob_line), |value| global.set_knob_line_commands(value));

    let guides = interaction::current_guides(store);
    let by_style = |style: GuideStyle| path(guides.iter().filter(|guide| guide.style == style).map(|guide| (interaction::scene_to_stage(store, guide.from), interaction::scene_to_stage(store, guide.to))));
    set_if_changed(global.get_guides_normal(), SharedString::from(by_style(GuideStyle::Normal)), |value| global.set_guides_normal(value));
    set_if_changed(global.get_guides_bold(), SharedString::from(by_style(GuideStyle::Bold)), |value| global.set_guides_bold(value));
    set_if_changed(global.get_guides_dashed(), SharedString::from(by_style(GuideStyle::Dashed)), |value| global.set_guides_dashed(value));
    let labels: Vec<GuideLabelView> = guides
        .iter()
        .filter_map(|guide| {
            let text = guide.label.clone()?;
            let middle = interaction::scene_to_stage(store, ((guide.from.0 + guide.to.0) / 2.0, (guide.from.1 + guide.to.1) / 2.0));
            Some(GuideLabelView { x: middle.0 as f32, y: middle.1 as f32, text: text.into(), vertical: guide.from.0 == guide.to.0 })
        })
        .collect();
    replace_model(global.get_guide_labels(), labels, |model| global.set_guide_labels(model));

    let marquee = interaction::marquee_rect(store);
    set_if_changed(global.get_marquee_shown(), marquee.is_some(), |value| global.set_marquee_shown(value));
    if let Some(rect) = marquee {
        global.set_marquee_x(rect.left as f32);
        global.set_marquee_y(rect.top as f32);
        global.set_marquee_width(rect.width as f32);
        global.set_marquee_height(rect.height as f32);
    }
    let badges: Vec<BadgeView> = interaction::badges(store)
        .into_iter()
        .map(|badge| BadgeView {
            x: badge.left as f32,
            y: badge.top as f32,
            rotation: badge.rotation as f32,
            label: ovrley_app::lib::widget::widget_type_name(&badge.kind, store.i18n.language).into(),
            id: badge.id.as_str().into(),
            map: badge.camera_mode.is_some(),
            fixed: badge.camera_mode.as_deref() == Some("fixed"),
            hint_x: badge.bottom_center.0 as f32,
            hint_y: badge.bottom_center.1 as f32,
        })
        .collect();
    replace_model(global.get_badges(), badges, |model| global.set_badges(model));
    let cursor = match interaction::cursor(store) {
        interaction::EditorCursor::Default => EditorCursor::Default,
        interaction::EditorCursor::Move => EditorCursor::Move,
        interaction::EditorCursor::ResizeNs => EditorCursor::ResizeNs,
        interaction::EditorCursor::ResizeEw => EditorCursor::ResizeEw,
        interaction::EditorCursor::ResizeNwse => EditorCursor::ResizeNwse,
        interaction::EditorCursor::ResizeNesw => EditorCursor::ResizeNesw,
        interaction::EditorCursor::Rotate => EditorCursor::Rotate,
    };
    set_if_changed(global.get_cursor(), cursor, |value| global.set_cursor(value));
}

/// A closed outline as dashes of `dash` stage pixels with equal gaps, continuing round its corners.
fn dashed_outline(corners: &[(f64, f64); 4], dash: f64) -> String {
    let mut commands = String::new();
    let mut phase = 0.0;
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        let length = (to.0 - from.0).hypot(to.1 - from.1);
        let mut along = 0.0;
        while along < length {
            let period = phase % (2.0 * dash);
            let step = if period < dash { dash - period } else { 2.0 * dash - period }.min(length - along);
            if period < dash {
                let at = |t: f64| (from.0 + (to.0 - from.0) * t / length, from.1 + (to.1 - from.1) * t / length);
                let (start, end) = (at(along), at(along + step));
                commands.push_str(&format!("M {} {} L {} {} ", start.0, start.1, end.0, end.1));
            }
            along += step;
            phase += step;
        }
    }
    commands
}

fn modifiers(alt: bool, control: bool, shift: bool, meta: bool) -> PointerModifiers {
    // The browser's `ctrlKey` and `metaKey`, as the key adapter maps them per platform.
    let input = key_input("", SlintModifiers { alt, control, shift, meta }, false, cfg!(target_os = "macos"));
    PointerModifiers { shift: input.shift, ctrl: input.ctrl, meta: input.meta, alt: input.alt }
}

/// Scroll-area coordinates to stage coordinates.
fn stage_point(store: &Store, x: f32, y: f32) -> (f64, f64) {
    let viewport = &store.overlay_editor.viewport;
    (f64::from(x) + viewport.scroll_x, f64::from(y) + viewport.scroll_y)
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<OverlayEditor>();
    let dispatch = dispatcher.handle();
    let weak = window.as_weak();
    global.on_viewport_changed(move |editor_width, editor_height, scroll_width, scroll_height, padding, rem| {
        let scale_factor = weak.upgrade().map_or(1.0, |window| f64::from(window.window().scale_factor()));
        let metrics = ViewportMetrics {
            editor_width: f64::from(editor_width),
            editor_height: f64::from(editor_height),
            scroll_width: f64::from(scroll_width),
            scroll_height: f64::from(scroll_height),
            padding: f64::from(padding),
            scale_factor,
            rem: f64::from(rem),
        };
        dispatch.dispatch(move |store, _| {
            if store.overlay_editor.viewport.metrics != metrics {
                editor::set_viewport_metrics(store, metrics);
            }
        });
    });
    let dispatch = dispatcher.handle();
    global.on_wheel(move |delta_x, delta_y, x, y, alt, control, shift, meta| {
        let input = key_input("wheel", SlintModifiers { alt, control, shift, meta }, false, cfg!(target_os = "macos"));
        // Slint reports a wheel turned towards the user as negative; the browser's `deltaY` is
        // positive then.
        let wheel = WheelInput {
            delta_x: -f64::from(delta_x),
            delta_y: -f64::from(delta_y),
            cursor: (f64::from(x), f64::from(y)),
        };
        let pointer = modifiers(alt, control, shift, meta);
        dispatch.dispatch(move |store, services| {
            // A map in view mode zooms with an unmodified wheel over it (phase 5 D10).
            if ovrley_app::features::maps::wheel(store, &wheel, pointer, std::time::Instant::now()) {
                present_pending_frame(store, services);
                return;
            }
            editor::handle_wheel(store, &input, wheel)
        });
    });
    let dispatch = dispatcher.handle();
    global.on_pointer_down(move |x, y, alt, control, shift, meta| {
        let modifiers = modifiers(alt, control, shift, meta);
        dispatch.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            interaction::pointer_down(store, point, modifiers);
        });
    });
    let dispatch = dispatcher.handle();
    global.on_pointer_move(move |x, y, alt, control, shift, meta| {
        let modifiers = modifiers(alt, control, shift, meta);
        dispatch.dispatch(move |store, services| {
            let point = stage_point(store, x, y);
            interaction::pointer_move(store, point, modifiers);
            // The moved widgets present in this action, so their images and the selection
            // outline reach the same rendered frame instead of the images trailing by a turn.
            present_pending_frame(store, services);
        });
    });
    let dispatch = dispatcher.handle();
    global.on_pointer_up(move |x, y, alt, control, shift, meta| {
        let modifiers = modifiers(alt, control, shift, meta);
        dispatch.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            interaction::pointer_up(store, point, modifiers);
        });
    });
    let dispatch = dispatcher.handle();
    global.on_pointer_leave(move || dispatch.dispatch(interaction::pointer_leave_action));
    let dispatch = dispatcher.handle();
    global.on_double_click(move |x, y| {
        dispatch.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            ovrley_app::features::maps::double_click(store, point);
        });
    });
    let dispatch = dispatcher.handle();
    global.on_reset_map_view(move |id| {
        let id = id.to_string();
        dispatch.dispatch(move |store, _| {
            if let Err(error) = ovrley_app::features::maps::reset_map_view(store, &id) {
                log::error!("The map view could not reset: {error}");
            }
        });
    });
    let dispatch = dispatcher.handle();
    global.on_zoom_in(move || dispatch.dispatch(|store, _| editor::set_zoom(store, step_zoom(store, true), None)));
    let dispatch = dispatcher.handle();
    global.on_zoom_out(move || dispatch.dispatch(|store, _| editor::set_zoom(store, step_zoom(store, false), None)));
    let dispatch = dispatcher.handle();
    global.on_reset_zoom(move || dispatch.dispatch(|store, _| editor::set_zoom(store, 1.0, None)));
    let dispatch = dispatcher.handle();
    global.on_set_background_mode(move |mode| {
        let mode = from_slint(mode);
        dispatch.dispatch(move |store, _| store.editor_shell.set_editor_background_mode(mode));
    });
    let dispatch = dispatcher.handle();
    global.on_set_grid_visible(move |visible| dispatch.dispatch(move |store, _| store.editor_shell.set_editor_grid_visible(visible)));
    let dispatch = dispatcher.handle();
    global.on_set_snap_to_grid(move |snap| dispatch.dispatch(move |store, _| store.editor_shell.set_editor_snap_to_grid(snap)));
}

/// The toolbar's zoom steps, through `useEditorShellState`'s rounding.
fn step_zoom(store: &Store, increase: bool) -> f64 {
    let mut shell = store.editor_shell;
    if increase {
        shell.increase_zoom();
    } else {
        shell.decrease_zoom();
    }
    shell.editor_zoom_level
}
