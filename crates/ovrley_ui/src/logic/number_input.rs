use crate::{InlineBlurKind, NumberInputLogic, SteppedNumber};
use ovrley_app::hooks::inline_number_input::{self as inline, InlineBlur};
use ovrley_app::lib::number_input::{self as ni, Step, StepDirection};

fn constraints(c: crate::NumberConstraints) -> ni::NumberConstraints {
    ni::NumberConstraints {
        min: c.has_min.then_some(c.min as f64),
        max: c.has_max.then_some(c.max as f64),
        step: Step::parse(&c.step),
    }
}

pub(super) fn bind(logic: &NumberInputLogic) {
    logic.on_is_number_input_text(|text| ni::is_number_input_text(&text));
    logic.on_step(|text, c, direction| {
        let direction = match direction {
            crate::StepDirection::Up => StepDirection::Up,
            crate::StepDirection::Down => StepDirection::Down,
        };
        match ni::step_value(&text, &constraints(c), direction) {
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
    logic.on_inline_blur(|text, value, c| {
        match inline::blur(&text, value as f64, &constraints(c)) {
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
