//! The component logic globals (`ui/components/logic.slint`), bound to the UI-free logic in
//! `ovrley_app`. Each binder converts between Slint's types and the app's and nothing else.

use crate::projection::{convert_enum, hex_color};
use crate::{
    ChoiceKind, ChoiceListLogic, ChoiceListMetrics, ColorFormat, ColorPickerLogic, GradientPickerLogic,
    GradientStopKeyOutcome, GradientStopKeyResult, GradientTrackSegment, Hsl, Hsv, InlineBlurKind,
    ItemAlignedPlacement, NumberInputLogic, ParsedChannel, ParsedColor, Placement, PlacementAlign,
    PlacementLogic, PlacementRect, PlacementSide, Rgba, StepDirection, SteppedNumber,
};
use ovrley_app::controls::gradient::{self as gp, StopKey, StopKeyResult};
use ovrley_app::controls::number::{self as inline, InlineBlur};
use ovrley_app::controls::choice::{self as cl, ChoiceKind as AppChoiceKind};
use ovrley_app::controls::color::{self as cp, ColorFormat as AppColorFormat};
use ovrley_app::controls::gradient::{self as gs, GradientStop};
use ovrley_app::controls::number::{self as ni, Step, StepDirection as AppStepDirection};
use ovrley_app::controls::placement::{self as pl, Align, Rect, Side, Size};
use ovrley_widgets::model::document::normalize_hex_color;
use slint::platform::Key;
use slint::{ComponentHandle, Global, Model, ModelRc, SharedString, VecModel};

/// Installs every logic global on a window. The app window and the test harnesses run it at start.
pub fn install_component_logic<C>(window: &C)
where
    C: ComponentHandle,
    for<'a> ChoiceListLogic<'a>: Global<'a, C>,
    for<'a> ColorPickerLogic<'a>: Global<'a, C>,
    for<'a> GradientPickerLogic<'a>: Global<'a, C>,
    for<'a> NumberInputLogic<'a>: Global<'a, C>,
    for<'a> PlacementLogic<'a>: Global<'a, C>,
{
    bind_choice_list(&window.global::<ChoiceListLogic>());
    bind_color_picker(&window.global::<ColorPickerLogic>());
    bind_gradient_picker(&window.global::<GradientPickerLogic>());
    bind_number_input(&window.global::<NumberInputLogic>());
    bind_placement(&window.global::<PlacementLogic>());
}

convert_enum!(ChoiceKind <=> AppChoiceKind { Item, Label, Separator });
convert_enum!(ColorFormat <=> AppColorFormat { Hex, Rgb, Hsl, Hsb });
convert_enum!(StepDirection <=> AppStepDirection { Up, Down });
convert_enum!(PlacementSide <=> Side { Top, Right, Bottom, Left });
convert_enum!(PlacementAlign <=> Align { Start, Center, End });

/// An index into a list, -1 for none.
fn list_index(index: Option<usize>) -> i32 {
    index.map_or(-1, |index| {
        i32::try_from(index).expect("a list index fits an i32")
    })
}

fn choices(items: &ModelRc<crate::Choice>) -> Vec<cl::Choice> {
    items
        .iter()
        .map(|item| cl::Choice {
            kind: item.kind.into(),
            value: item.value.to_string(),
            label: item.label.to_string(),
            disabled: item.disabled,
        })
        .collect()
}

fn bind_choice_list(logic: &ChoiceListLogic) {
    logic.on_index_of(|items, value| list_index(cl::index_of(&choices(&items), &value)));
    logic.on_next_enabled(|items, from, direction, wrap| {
        list_index(cl::next_selectable(
            &choices(&items),
            from as isize,
            direction as isize,
            wrap,
        ))
    });
    logic.on_typeahead(|items, current, search| {
        list_index(cl::typeahead(
            &choices(&items),
            usize::try_from(current).ok(),
            &search,
        ))
    });
    logic.on_list_metrics(|items, index, row, separator, padding| {
        let entry = usize::try_from(index).unwrap_or(0);
        let metrics = cl::list_metrics(
            &choices(&items),
            entry,
            f64::from(row),
            f64::from(separator),
            f64::from(padding),
        );
        ChoiceListMetrics {
            content_height: metrics.content_height as f32,
            entry_top: metrics.entry_top as f32,
        }
    });
    logic.on_font_select_choices(|recommended, recommended_label, system, system_label| {
        let label = |label: SharedString| crate::Choice {
            kind: ChoiceKind::Label,
            label,
            ..Default::default()
        };
        let separator = crate::Choice {
            kind: ChoiceKind::Separator,
            ..Default::default()
        };
        let recommended: Vec<_> = recommended.iter().collect();
        let system: Vec<_> = system.iter().collect();
        ModelRc::new(VecModel::from(cl::font_select_choices(
            &recommended,
            label(recommended_label),
            &system,
            label(system_label),
            separator,
        )))
    });
}

impl From<Rgba> for cp::Rgba {
    fn from(color: Rgba) -> Self {
        Self {
            r: color.r.into(),
            g: color.g.into(),
            b: color.b.into(),
            a: color.a.into(),
        }
    }
}

impl From<cp::Rgba> for Rgba {
    fn from(color: cp::Rgba) -> Self {
        Self {
            r: color.r as f32,
            g: color.g as f32,
            b: color.b as f32,
            a: color.a as f32,
        }
    }
}

fn bind_color_picker(logic: &ColorPickerLogic) {
    logic.on_hex_to_rgb(|hex, alpha| cp::hex_to_rgb(&hex, alpha.into()).into());
    logic.on_rgb_to_hex(|color| cp::rgb_to_hex(color.into()).into());
    logic.on_rgb_to_hsv(|color| {
        let hsv = cp::rgb_to_hsv(color.into());
        Hsv {
            h: hsv.h as f32,
            s: hsv.s as f32,
            v: hsv.v as f32,
            a: hsv.a as f32,
        }
    });
    logic.on_hsv_to_rgb(|hsv| {
        cp::hsv_to_rgb(cp::Hsv {
            h: hsv.h.into(),
            s: hsv.s.into(),
            v: hsv.v.into(),
            a: hsv.a.into(),
        })
        .into()
    });
    logic.on_rgb_to_hsl(|color| {
        let hsl = cp::rgb_to_hsl(color.into());
        Hsl {
            h: hsl.h as f32,
            s: hsl.s as f32,
            l: hsl.l as f32,
        }
    });
    logic.on_hsl_to_rgb(|hsl, alpha| {
        cp::hsl_to_rgb(
            cp::Hsl {
                h: hsl.h.into(),
                s: hsl.s.into(),
                l: hsl.l.into(),
            },
            alpha.into(),
        )
        .into()
    });
    logic.on_color_to_string(|color, format| {
        cp::color_to_string(color.into(), format.into()).into()
    });
    logic.on_parse_color_string(|value| match cp::parse_color_string(&value) {
        Some(color) => ParsedColor {
            valid: true,
            color: color.into(),
        },
        None => ParsedColor {
            valid: false,
            color: Rgba::default(),
        },
    });
    logic.on_parse_channel(|text, max| match cp::parse_channel(&text, max.into()) {
        Some(value) => ParsedChannel {
            valid: true,
            value: value as f32,
        },
        None => ParsedChannel {
            valid: false,
            value: 0.0,
        },
    });
    logic.on_normalize_hex_color(|value, fallback| normalize_hex_color(&value, &fallback).into());
    logic.on_hex_to_color(|hex| hex_color(&hex));
}

fn gradient_stops(stops: &ModelRc<crate::GradientStop>) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|stop| GradientStop {
            offset: stop.offset.into(),
            color: stop.color.to_string(),
        })
        .collect()
}

fn gradient_stops_model(stops: Vec<GradientStop>) -> ModelRc<crate::GradientStop> {
    let stops: Vec<crate::GradientStop> = stops
        .into_iter()
        .map(|stop| crate::GradientStop {
            offset: stop.offset as f32,
            color: stop.color.into(),
        })
        .collect();
    ModelRc::new(VecModel::from(stops))
}

/// A stop's index.
///
/// # Panics
///
/// On a negative index: the picker passes the index of a stop it shows.
fn stop_index(index: i32) -> usize {
    usize::try_from(index)
        .unwrap_or_else(|_| panic!("gradient stop index must be non-negative, got {index}"))
}

fn stop_key(text: &SharedString) -> StopKey {
    let is = |key: Key| *text == SharedString::from(key);
    if is(Key::Delete) || is(Key::Backspace) {
        StopKey::Delete
    } else if is(Key::LeftArrow) {
        StopKey::ArrowLeft
    } else if is(Key::RightArrow) {
        StopKey::ArrowRight
    } else {
        StopKey::Other
    }
}

fn bind_gradient_picker(logic: &GradientPickerLogic) {
    logic.on_track_segments(|stops| {
        let segments: Vec<GradientTrackSegment> = gs::track_segments(&gradient_stops(&stops))
            .into_iter()
            .map(|segment| GradientTrackSegment {
                start: segment.start as f32,
                end: segment.end as f32,
                start_color: hex_color(&segment.start_color),
                end_color: hex_color(&segment.end_color),
            })
            .collect();
        ModelRc::new(VecModel::from(segments))
    });
    logic.on_can_add_stop(|count| gp::can_add_stop(usize::try_from(count).unwrap_or(0)));
    logic.on_can_remove_stop(|count| gp::can_remove_stop(usize::try_from(count).unwrap_or(0)));
    logic.on_offset_at(|x, width| gp::offset_at(x.into(), width.into()) as f32);
    logic.on_exceeds_drag_threshold(|start, x| gp::exceeds_drag_threshold(start.into(), x.into()));
    logic.on_add_stop(|stops, offset| {
        gradient_stops_model(gs::add_gradient_stop(
            &gradient_stops(&stops),
            offset.into(),
        ))
    });
    logic.on_move_stop(|stops, index, offset| {
        gradient_stops_model(gs::move_gradient_stop(
            &gradient_stops(&stops),
            stop_index(index),
            offset.into(),
        ))
    });
    logic.on_set_stop_color(|stops, index, color| {
        gradient_stops_model(gs::set_gradient_stop_color(
            &gradient_stops(&stops),
            stop_index(index),
            &color,
        ))
    });
    logic.on_remove_stop(|stops, index| {
        gradient_stops_model(gs::remove_gradient_stop(
            &gradient_stops(&stops),
            stop_index(index),
        ))
    });
    logic.on_stop_key(|stops, index, key, shift| {
        match gp::stop_key(
            &gradient_stops(&stops),
            stop_index(index),
            stop_key(&key),
            shift,
        ) {
            StopKeyResult::Ignored => GradientStopKeyOutcome {
                result: GradientStopKeyResult::Ignored,
                stops,
            },
            StopKeyResult::Handled => GradientStopKeyOutcome {
                result: GradientStopKeyResult::Handled,
                stops,
            },
            StopKeyResult::Apply(next) => GradientStopKeyOutcome {
                result: GradientStopKeyResult::Apply,
                stops: gradient_stops_model(next),
            },
        }
    });
}

impl From<crate::NumberConstraints> for ni::NumberConstraints {
    fn from(constraints: crate::NumberConstraints) -> Self {
        Self {
            min: constraints.has_min.then_some(constraints.min.into()),
            max: constraints.has_max.then_some(constraints.max.into()),
            step: Step::parse(&constraints.step),
        }
    }
}

fn bind_number_input(logic: &NumberInputLogic) {
    logic.on_is_number_input_text(|text| ni::is_number_input_text(&text));
    logic.on_step(|text, constraints, direction| {
        match ni::step_value(&text, &constraints.into(), direction.into()) {
            Some(next) => SteppedNumber {
                stepped: true,
                text: next.into(),
            },
            None => SteppedNumber {
                stepped: false,
                text,
            },
        }
    });
    logic.on_inline_blur(|text, value, constraints| {
        match inline::blur(&text, value.into(), &constraints.into()) {
            InlineBlur::Unchanged => crate::InlineBlur {
                kind: InlineBlurKind::Unchanged,
                value,
            },
            InlineBlur::Commit(next) => crate::InlineBlur {
                kind: InlineBlurKind::Commit,
                value: next as f32,
            },
            InlineBlur::Invalid => crate::InlineBlur {
                kind: InlineBlurKind::Invalid,
                value,
            },
        }
    });
}

impl From<PlacementRect> for Rect {
    fn from(rect: PlacementRect) -> Self {
        Self {
            x: rect.x.into(),
            y: rect.y.into(),
            width: rect.width.into(),
            height: rect.height.into(),
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size {
        width: width.into(),
        height: height.into(),
    }
}

fn bind_placement(logic: &PlacementLogic) {
    logic.on_place(
        |anchor,
         content_width,
         content_height,
         window_width,
         window_height,
         side,
         align,
         offset| {
            let placed = pl::place(
                anchor.into(),
                size(content_width, content_height),
                size(window_width, window_height),
                side.into(),
                align.into(),
                offset.into(),
            );
            Placement {
                x: placed.x as f32,
                y: placed.y as f32,
                side: placed.side.into(),
                available_height: placed.available_height as f32,
            }
        },
    );
    logic.on_item_aligned(
        |trigger,
         content_width,
         content_height,
         window_width,
         window_height,
         item_top,
         item_height| {
            let placed = pl::item_aligned(
                trigger.into(),
                size(content_width, content_height),
                size(window_width, window_height),
                item_top.into(),
                item_height.into(),
            );
            ItemAlignedPlacement {
                x: placed.x as f32,
                y: placed.y as f32,
                width: placed.width as f32,
                height: placed.height as f32,
                scroll_y: placed.scroll_y as f32,
            }
        },
    );
}
