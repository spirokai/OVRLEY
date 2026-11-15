//! The overlay-editor feature: its projection into the `OverlayEditor` global, the widget images
//! (D10), the preview frame's scheduling (D9) and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::platform::key_input::{key_input, SlintModifiers};
use crate::{AppWindow, BackgroundMode, OverlayEditor, WidgetView};
use ovrley_app::features::app_shell::EditorBackgroundMode;
use ovrley_app::features::overlay_editor::{self as editor, viewport::ViewportMetrics, WheelInput};
use ovrley_app::features::widget_preview::{preview_frame, widget_layer};
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
        }
        set_if_changed(global.get_snap_to_grid(), shell.editor_snap_to_grid, |value| global.set_snap_to_grid(value));
        let zoom = format!("{}%", (shell.editor_zoom_level * 100.0).round());
        set_if_changed(global.get_zoom_label(), SharedString::from(zoom), |value| global.set_zoom_label(value));
        let error = store.preview.error.clone().unwrap_or_default();
        set_if_changed(global.get_error(), SharedString::from(error), |value| global.set_error(value));

        self.project_widgets(&global, store, display);
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
        let current = global.get_widgets();
        let same = current.row_count() == next.len() && current.iter().zip(&next).all(|(left, right)| left == *right);
        if !same {
            global.set_widgets(ModelRc::new(VecModel::from(next)));
        }
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<OverlayEditor>();
    let dispatch = dispatcher.handle();
    let weak = window.as_weak();
    global.on_viewport_changed(move |editor_width, editor_height, scroll_width, scroll_height, padding| {
        let scale_factor = weak.upgrade().map_or(1.0, |window| f64::from(window.window().scale_factor()));
        let metrics = ViewportMetrics {
            editor_width: f64::from(editor_width),
            editor_height: f64::from(editor_height),
            scroll_width: f64::from(scroll_width),
            scroll_height: f64::from(scroll_height),
            padding: f64::from(padding),
            scale_factor,
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
        dispatch.dispatch(move |store, _| editor::handle_wheel(store, &input, wheel));
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
