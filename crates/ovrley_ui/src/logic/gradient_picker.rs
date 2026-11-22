use crate::{
    GradientPickerLogic, GradientStopKeyOutcome, GradientStopKeyResult, GradientTrackSegment,
};
use ovrley_app::hooks::gradient_picker::{self as gp, StopKey, StopKeyResult};
use ovrley_app::lib::gradient_stops::{self as gs, GradientStop};
use slint::platform::Key;
use slint::{Model, ModelRc, SharedString, VecModel};

use super::color_picker::hex_to_color;

fn to_app(stops: &ModelRc<crate::GradientStop>) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|stop| GradientStop {
            offset: stop.offset as f64,
            color: stop.color.to_string(),
        })
        .collect()
}

fn to_ui(stops: Vec<GradientStop>) -> ModelRc<crate::GradientStop> {
    ModelRc::new(VecModel::from(
        stops
            .into_iter()
            .map(|stop| crate::GradientStop {
                offset: stop.offset as f32,
                color: stop.color.into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn index(index: i32) -> usize {
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

pub(super) fn bind(logic: &GradientPickerLogic) {
    logic.on_track_segments(|stops| {
        let segments: Vec<GradientTrackSegment> = gs::track_segments(&to_app(&stops))
            .into_iter()
            .map(|segment| GradientTrackSegment {
                start: segment.start as f32,
                end: segment.end as f32,
                start_color: hex_to_color(&segment.start_color),
                end_color: hex_to_color(&segment.end_color),
            })
            .collect();
        ModelRc::new(VecModel::from(segments))
    });
    logic.on_can_add_stop(|count| gp::can_add_stop(count as usize));
    logic.on_can_remove_stop(|count| gp::can_remove_stop(count as usize));
    logic.on_offset_at(|x, width| gp::offset_at(x as f64, width as f64) as f32);
    logic.on_exceeds_drag_threshold(|start, x| gp::exceeds_drag_threshold(start as f64, x as f64));
    logic.on_add_stop(|stops, offset| to_ui(gs::add_gradient_stop(&to_app(&stops), offset as f64)));
    logic.on_move_stop(|stops, i, offset| {
        to_ui(gs::move_gradient_stop(
            &to_app(&stops),
            index(i),
            offset as f64,
        ))
    });
    logic.on_set_stop_color(|stops, i, color| {
        to_ui(gs::set_gradient_stop_color(
            &to_app(&stops),
            index(i),
            &color,
        ))
    });
    logic.on_remove_stop(|stops, i| to_ui(gs::remove_gradient_stop(&to_app(&stops), index(i))));
    logic.on_stop_key(|stops, i, key, shift| {
        match gp::stop_key(&to_app(&stops), index(i), stop_key(&key), shift) {
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
                stops: to_ui(next),
            },
        }
    });
}
