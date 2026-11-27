//! The overlay editor: the `OverlayEditor` global (stage, background, grid, selection layer), the
//! widget images, the preview frame's scheduling, the maps' deadline timer, and the pointer, wheel
//! and toolbar callbacks.

use crate::platform::keys::Modifiers;
use crate::projection::{convert_enum, hex_color, premultiplied_image, replace_if_changed, set_if_changed};
use crate::{
    AppWindow, BackgroundMode, BadgeView, EditorCursor, GuideLabelView, HandleView, OverlayEditor,
    WidgetView,
};
use ovrley_app::features::shell::{EditorBackgroundMode, EditorShellState};
use ovrley_app::features::maps;
use ovrley_app::features::canvas::geometry::GuideStyle;
use ovrley_app::features::canvas::interaction::{
    self, EditorCursor as AppEditorCursor, Handle,
};
use ovrley_app::features::canvas::{self as editor, viewport::ViewportMetrics, WheelInput};
use ovrley_app::features::widgets::preview::{present_pending_frame, preview_frame, widget_layer};
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use slint::{ComponentHandle, Image, SharedString};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

convert_enum!(BackgroundMode <=> EditorBackgroundMode { Checker, Black, White, Image, Video });
convert_enum!(EditorCursor <=> AppEditorCursor { Default, Move, ResizeNs, ResizeEw, ResizeNwse, ResizeNesw, Rotate });

/// A widget's image and the pixels it was made from.
struct WidgetImage {
    revision: u32,
    handle: u32,
    image: Image,
}

/// The projection's memory: each widget's image, replaced only when its pixels change; the
/// background image's path, so it loads once per path; whether a preview frame is scheduled; and
/// the maps' next deadline.
pub struct Projection {
    images: RefCell<HashMap<String, WidgetImage>>,
    background_image_path: RefCell<Option<String>>,
    frame_scheduled: Rc<Cell<bool>>,
    maps_timer: slint::Timer,
    maps_deadline: Cell<Option<Instant>>,
    dispatch: DispatchHandle,
}

impl Projection {
    pub fn new(dispatch: DispatchHandle) -> Self {
        Self {
            images: RefCell::default(),
            background_image_path: RefCell::default(),
            frame_scheduled: Rc::default(),
            maps_timer: slint::Timer::default(),
            maps_deadline: Cell::new(None),
            dispatch,
        }
    }

    /// One preview frame on the next event-loop turn, however many actions asked for one.
    fn schedule_frame(&self, store: &Store) {
        if !store.preview.is_dirty() || self.frame_scheduled.replace(true) {
            return;
        }
        let scheduled = Rc::clone(&self.frame_scheduled);
        let dispatch = self.dispatch.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            scheduled.set(false);
            dispatch.dispatch(preview_frame);
        });
    }

    /// Comes back at the maps' next deadline: a wheel's zoom commit, the settled route download.
    fn schedule_maps(&self, store: &Store) {
        let deadline = maps::next_deadline(store);
        if deadline == self.maps_deadline.replace(deadline) {
            return;
        }
        let Some(deadline) = deadline else {
            self.maps_timer.stop();
            return;
        };
        let dispatch = self.dispatch.clone();
        self.maps_timer.start(
            slint::TimerMode::SingleShot,
            deadline.saturating_duration_since(Instant::now()),
            move || {
                dispatch.dispatch(|store, services| maps::tick(store, services, Instant::now()));
            },
        );
    }

    pub fn project(&self, window: &AppWindow, store: &Store) {
        let global = window.global::<OverlayEditor>();
        let (scene_width, scene_height) = editor::scene_size(store);
        let display = editor::display_scale(store);
        let stage = editor::stage(store);
        let px = |value: f64| value as f32;
        set_if_changed(global.get_stage_width(), px(stage.content_width), |value| {
            global.set_stage_width(value)
        });
        set_if_changed(
            global.get_stage_height(),
            px(stage.content_height),
            |value| global.set_stage_height(value),
        );
        set_if_changed(global.get_scene_left(), px(stage.scene_left), |value| {
            global.set_scene_left(value)
        });
        set_if_changed(global.get_scene_top(), px(stage.scene_top), |value| {
            global.set_scene_top(value)
        });
        set_if_changed(global.get_scene_width(), px(stage.scene_width), |value| {
            global.set_scene_width(value)
        });
        set_if_changed(global.get_scene_height(), px(stage.scene_height), |value| {
            global.set_scene_height(value)
        });
        let viewport = &store.canvas.viewport;
        set_if_changed(global.get_scroll_x(), px(viewport.scroll_x), |value| {
            global.set_scroll_x(value)
        });
        set_if_changed(global.get_scroll_y(), px(viewport.scroll_y), |value| {
            global.set_scroll_y(value)
        });
        set_if_changed(global.get_display_scale(), px(display), |value| {
            global.set_display_scale(value)
        });
        let label = format!("{scene_width} × {scene_height}");
        set_if_changed(
            global.get_scene_size_label(),
            SharedString::from(label),
            |value| global.set_scene_size_label(value),
        );

        let shell = &store.editor_shell;
        set_if_changed(
            global.get_background_mode(),
            shell.editor_background_mode.into(),
            |value| global.set_background_mode(value),
        );
        let background = hex_color(editor::canvas_background_color(
            shell.editor_background_mode,
        ));
        set_if_changed(global.get_background_color(), background, |value| {
            global.set_background_color(value)
        });
        let image_path = store.video_import.imported_background_image_path.as_deref();
        set_if_changed(
            global.get_has_background_image(),
            image_path.is_some(),
            |value| global.set_has_background_image(value),
        );
        set_if_changed(
            global.get_has_videos(),
            store.video_import.has_videos(),
            |value| global.set_has_videos(value),
        );
        if self.background_image_path.borrow().as_deref() != image_path {
            *self.background_image_path.borrow_mut() = image_path.map(str::to_string);
            // A file that cannot be read shows the black background.
            let image = image_path
                .and_then(|path| Image::load_from_path(std::path::Path::new(path)).ok())
                .unwrap_or_default();
            global.set_background_image(image);
        }
        set_if_changed(
            global.get_grid_visible(),
            shell.editor_grid_visible,
            |value| global.set_grid_visible(value),
        );
        if shell.editor_grid_visible {
            let commands = editor::grid_commands((scene_width, scene_height), display);
            set_if_changed(
                global.get_grid_commands(),
                SharedString::from(commands),
                |value| global.set_grid_commands(value),
            );
        }
        set_if_changed(
            global.get_snap_to_grid(),
            shell.editor_snap_to_grid,
            |value| global.set_snap_to_grid(value),
        );
        let zoom = format!("{}%", (shell.editor_zoom_level * 100.0).round());
        set_if_changed(global.get_zoom_label(), SharedString::from(zoom), |value| {
            global.set_zoom_label(value)
        });
        let error = store.preview.error.clone().unwrap_or_default();
        set_if_changed(global.get_error(), SharedString::from(error), |value| {
            global.set_error(value)
        });

        self.project_widgets(&global, store, display);
        project_selection(&global, store);
        self.schedule_frame(store);
        self.schedule_maps(store);
    }

    /// The widget images in stacking order: layer, then document order.
    fn project_widgets(&self, global: &OverlayEditor<'_>, store: &Store, display: f64) {
        let presented = store.preview.presented();
        let mut images = self.images.borrow_mut();
        images.retain(|id, _| presented.contains_key(id));
        let mut views: Vec<(u8, usize, WidgetView)> = Vec::new();
        for (order, id) in store.editor.config.widget_ids().iter().enumerate() {
            let Some(widget) = presented.get(id) else {
                continue;
            };
            // Without activity at the playhead only rasters show; the others stay editable.
            if !store.preview.activity_coverage && widget.kind != "raster" {
                continue;
            }
            let geometry = &widget.geometry;
            let image = match images.get(id) {
                Some(cached)
                    if cached.revision == widget.pixels_revision
                        && cached.handle == widget.handle =>
                {
                    cached.image.clone()
                }
                _ => {
                    let image = premultiplied_image(
                        store.preview.pixels(widget.handle),
                        geometry.width,
                        geometry.height,
                    );
                    images.insert(
                        id.clone(),
                        WidgetImage {
                            revision: widget.pixels_revision,
                            handle: widget.handle,
                            image: image.clone(),
                        },
                    );
                    image
                }
            };
            let scale = f64::from(geometry.preview_raster_scale);
            views.push((
                widget_layer(&widget.kind),
                order,
                WidgetView {
                    id: id.into(),
                    image,
                    x: (f64::from(geometry.left) * display) as f32,
                    y: (f64::from(geometry.top) * display) as f32,
                    width: (f64::from(geometry.width) / scale * display) as f32,
                    height: (f64::from(geometry.height) / scale * display) as f32,
                },
            ));
        }
        views.sort_by_key(|(layer, order, _)| (*layer, *order));
        let next: Vec<WidgetView> = views.into_iter().map(|(_, _, view)| view).collect();
        replace_if_changed(global.get_widgets(), next, |model| {
            global.set_widgets(model)
        });
    }
}

/// Path commands for straight segments.
fn segments_path(segments: impl IntoIterator<Item = ((f64, f64), (f64, f64))>) -> String {
    let mut commands = String::new();
    for (from, to) in segments {
        let _ = write!(commands, "M {} {} L {} {} ", from.0, from.1, to.0, to.1);
    }
    commands.truncate(commands.trim_end().len());
    commands
}

/// The selection layer: outlines, handles, the knob line, guides, labels, the marquee and the
/// badges, in stage pixels.
fn project_selection(global: &OverlayEditor<'_>, store: &Store) {
    let metrics = &store.canvas.viewport.metrics;
    let handle_scale = editor::viewport::handle_scale(metrics);
    let moveable = 0.75 * handle_scale;
    // The map in view mode draws its outline dashed.
    let dashed = maps::map_view_widget(store).is_some();
    let corners = interaction::selection_outlines(store);
    let mut outlines = String::new();
    let mut dashes = String::new();
    for corners in &corners {
        if dashed {
            dashed_outline(&mut dashes, corners, 3.0);
        } else {
            let [a, b, c, d] = corners;
            let _ = write!(
                outlines,
                "M {} {} L {} {} L {} {} L {} {} Z ",
                a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1
            );
        }
    }
    set_if_changed(
        global.get_selection_commands(),
        SharedString::from(outlines.trim_end()),
        |value| global.set_selection_commands(value),
    );
    set_if_changed(
        global.get_selection_dashed_commands(),
        SharedString::from(dashes.trim_end()),
        |value| global.set_selection_dashed_commands(value),
    );
    let device_pixel = (1.0 / metrics.scale_factor.max(1.0)) as f32;
    set_if_changed(global.get_border_width(), device_pixel, |value| {
        global.set_border_width(value)
    });
    set_if_changed(global.get_guide_scale(), moveable as f32, |value| {
        global.set_guide_scale(value)
    });
    let rem = metrics.rem as f32;
    set_if_changed(
        global.get_handle_size(),
        0.741 * rem * moveable as f32,
        |value| global.set_handle_size(value),
    );
    set_if_changed(
        global.get_knob_size(),
        1.037 * rem * moveable as f32,
        |value| global.set_knob_size(value),
    );

    let rotation = interaction::selection_rotation(store) as f32;
    let mut knob_line = String::new();
    let handles: Vec<HandleView> = interaction::handles(store)
        .iter()
        .map(|(point, handle)| {
            let stage = interaction::scene_to_stage(store, *point);
            let knob = *handle == Handle::Rotate;
            if knob {
                if let Some(top) = interaction::selection_top_middle(store) {
                    knob_line = segments_path([(interaction::scene_to_stage(store, top), stage)]);
                }
            }
            HandleView {
                x: stage.0 as f32,
                y: stage.1 as f32,
                rotation,
                knob,
            }
        })
        .collect();
    replace_if_changed(global.get_handles(), handles, |model| {
        global.set_handles(model)
    });
    set_if_changed(
        global.get_knob_line_commands(),
        SharedString::from(knob_line),
        |value| global.set_knob_line_commands(value),
    );

    let guides = interaction::current_guides(store);
    let by_style = |style: GuideStyle| {
        let styled = guides.iter().filter(|guide| guide.style == style);
        SharedString::from(segments_path(styled.map(|guide| {
            (
                interaction::scene_to_stage(store, guide.from),
                interaction::scene_to_stage(store, guide.to),
            )
        })))
    };
    set_if_changed(
        global.get_guides_normal(),
        by_style(GuideStyle::Normal),
        |value| global.set_guides_normal(value),
    );
    set_if_changed(
        global.get_guides_bold(),
        by_style(GuideStyle::Bold),
        |value| global.set_guides_bold(value),
    );
    set_if_changed(
        global.get_guides_dashed(),
        by_style(GuideStyle::Dashed),
        |value| global.set_guides_dashed(value),
    );
    let labels: Vec<GuideLabelView> = guides
        .iter()
        .filter_map(|guide| {
            let text = guide.label.clone()?;
            let middle = interaction::scene_to_stage(
                store,
                (
                    f64::midpoint(guide.from.0, guide.to.0),
                    f64::midpoint(guide.from.1, guide.to.1),
                ),
            );
            Some(GuideLabelView {
                x: middle.0 as f32,
                y: middle.1 as f32,
                text: text.into(),
                vertical: guide.from.0 == guide.to.0,
            })
        })
        .collect();
    replace_if_changed(global.get_guide_labels(), labels, |model| {
        global.set_guide_labels(model)
    });

    let marquee = interaction::marquee_rect(store);
    set_if_changed(global.get_marquee_shown(), marquee.is_some(), |value| {
        global.set_marquee_shown(value)
    });
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
            label: ovrley_app::features::widgets::widget_type_name(&badge.kind, store.i18n.language)
                .into(),
            id: badge.id.as_str().into(),
            map: badge.camera_mode.is_some(),
            fixed: badge.camera_mode.as_deref() == Some("fixed"),
            hint_x: badge.bottom_center.0 as f32,
            hint_y: badge.bottom_center.1 as f32,
        })
        .collect();
    replace_if_changed(global.get_badges(), badges, |model| {
        global.set_badges(model)
    });
    set_if_changed(
        global.get_cursor(),
        interaction::cursor(store).into(),
        |value| global.set_cursor(value),
    );
}

/// Appends a closed outline as dashes of `dash` stage pixels with equal gaps, continuing round its
/// corners.
fn dashed_outline(commands: &mut String, corners: &[(f64, f64); 4], dash: f64) {
    let mut phase = 0.0;
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        let length = (to.0 - from.0).hypot(to.1 - from.1);
        let at = |t: f64| {
            (
                from.0 + (to.0 - from.0) * t / length,
                from.1 + (to.1 - from.1) * t / length,
            )
        };
        let mut along = 0.0;
        while along < length {
            let period = phase % (2.0 * dash);
            let drawing = period < dash;
            let step = if drawing {
                dash - period
            } else {
                2.0 * dash - period
            }
            .min(length - along);
            if drawing {
                let (start, end) = (at(along), at(along + step));
                let _ = write!(commands, "M {} {} L {} {} ", start.0, start.1, end.0, end.1);
            }
            along += step;
            phase += step;
        }
    }
}

/// Scroll-area coordinates to stage coordinates.
fn stage_point(store: &Store, x: f32, y: f32) -> (f64, f64) {
    let viewport = &store.canvas.viewport;
    (
        f64::from(x) + viewport.scroll_x,
        f64::from(y) + viewport.scroll_y,
    )
}

/// The zoom one toolbar step away, with the shell's rounding and limits.
fn stepped_zoom(store: &Store, step: fn(&mut EditorShellState)) -> f64 {
    let mut shell = store.editor_shell;
    step(&mut shell);
    shell.editor_zoom_level
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<OverlayEditor>();
    let d = dispatcher.handle();
    let weak = window.as_weak();
    global.on_viewport_changed(
        move |editor_width, editor_height, scroll_width, scroll_height, padding, rem| {
            let scale_factor = weak
                .upgrade()
                .map_or(1.0, |window| f64::from(window.window().scale_factor()));
            let metrics = ViewportMetrics {
                editor_width: editor_width.into(),
                editor_height: editor_height.into(),
                scroll_width: scroll_width.into(),
                scroll_height: scroll_height.into(),
                padding: padding.into(),
                scale_factor,
                rem: rem.into(),
            };
            d.dispatch(move |store, _| {
                if store.canvas.viewport.metrics != metrics {
                    editor::set_viewport_metrics(store, metrics);
                }
            });
        },
    );
    let d = dispatcher.handle();
    global.on_wheel(move |delta_x, delta_y, x, y, alt, control, shift, meta| {
        let modifiers = Modifiers {
            alt,
            control,
            shift,
            meta,
        };
        let input = modifiers.key_input("wheel");
        let pointer = modifiers.pointer();
        // Slint reports a wheel turned towards the user as negative; the editor takes it as positive.
        let wheel = WheelInput {
            delta_x: -f64::from(delta_x),
            delta_y: -f64::from(delta_y),
            cursor: (x.into(), y.into()),
        };
        d.dispatch(move |store, services| {
            // A map in view mode zooms with an unmodified wheel over it.
            if maps::wheel(store, &wheel, pointer, Instant::now()) {
                present_pending_frame(store, services);
                return;
            }
            editor::handle_wheel(store, &input, wheel);
        });
    });
    let d = dispatcher.handle();
    global.on_pointer_down(move |x, y, alt, control, shift, meta| {
        let modifiers = Modifiers {
            alt,
            control,
            shift,
            meta,
        }
        .pointer();
        d.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            interaction::pointer_down(store, point, modifiers);
        });
    });
    let d = dispatcher.handle();
    global.on_pointer_move(move |x, y, alt, control, shift, meta| {
        let modifiers = Modifiers {
            alt,
            control,
            shift,
            meta,
        }
        .pointer();
        d.dispatch(move |store, services| {
            let point = stage_point(store, x, y);
            interaction::pointer_move(store, point, modifiers);
            // The moved widgets present in this action, so their images and the selection
            // outline reach the same rendered frame instead of the images trailing by a turn.
            present_pending_frame(store, services);
        });
    });
    let d = dispatcher.handle();
    global.on_pointer_up(move |x, y, alt, control, shift, meta| {
        let modifiers = Modifiers {
            alt,
            control,
            shift,
            meta,
        }
        .pointer();
        d.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            interaction::pointer_up(store, point, modifiers);
        });
    });
    let d = dispatcher.handle();
    global.on_pointer_leave(move || d.dispatch(interaction::pointer_leave_action));
    let d = dispatcher.handle();
    global.on_double_click(move |x, y| {
        d.dispatch(move |store, _| {
            let point = stage_point(store, x, y);
            maps::double_click(store, point);
        });
    });
    let d = dispatcher.handle();
    global.on_reset_map_view(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, _| {
            if let Err(error) = maps::reset_map_view(store, &id) {
                log::error!("the map view could not reset: {error}");
            }
        });
    });
    let d = dispatcher.handle();
    global.on_zoom_in(move || {
        d.dispatch(|store, _| {
            editor::set_zoom(
                store,
                stepped_zoom(store, EditorShellState::increase_zoom),
                None,
            )
        })
    });
    let d = dispatcher.handle();
    global.on_zoom_out(move || {
        d.dispatch(|store, _| {
            editor::set_zoom(
                store,
                stepped_zoom(store, EditorShellState::decrease_zoom),
                None,
            )
        })
    });
    let d = dispatcher.handle();
    global.on_reset_zoom(move || d.dispatch(|store, _| editor::set_zoom(store, 1.0, None)));
    let d = dispatcher.handle();
    global.on_set_background_mode(move |mode| {
        d.dispatch(move |store, _| store.editor_shell.set_editor_background_mode(mode.into()))
    });
    let d = dispatcher.handle();
    global.on_set_grid_visible(move |visible| {
        d.dispatch(move |store, _| store.editor_shell.set_editor_grid_visible(visible))
    });
    let d = dispatcher.handle();
    global.on_set_snap_to_grid(move |snap| {
        d.dispatch(move |store, _| store.editor_shell.set_editor_snap_to_grid(snap))
    });
}
