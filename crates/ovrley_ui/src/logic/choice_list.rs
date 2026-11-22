use ovrley_app::lib::choice_list::{self as cl, Choice, ChoiceKind};
use slint::{Model, ModelRc, VecModel};

use crate::{ChoiceListLogic, ChoiceListMetrics};

fn kind(kind: crate::ChoiceKind) -> ChoiceKind {
    match kind {
        crate::ChoiceKind::Item => ChoiceKind::Item,
        crate::ChoiceKind::Label => ChoiceKind::Label,
        crate::ChoiceKind::Separator => ChoiceKind::Separator,
    }
}

fn choices(items: &ModelRc<crate::Choice>) -> Vec<Choice> {
    items
        .iter()
        .map(|item| Choice {
            kind: kind(item.kind),
            value: item.value.to_string(),
            label: item.label.to_string(),
            disabled: item.disabled,
        })
        .collect()
}

fn to_index(index: Option<usize>) -> i32 {
    index.map_or(-1, |index| index as i32)
}

fn entry(index: i32) -> usize {
    usize::try_from(index).unwrap_or(0)
}

pub(super) fn bind(logic: &ChoiceListLogic) {
    logic.on_index_of(|items, value| to_index(cl::index_of(&choices(&items), &value)));
    logic.on_next_enabled(|items, from, direction, wrap| {
        to_index(cl::next_selectable(
            &choices(&items),
            from as isize,
            direction as isize,
            wrap,
        ))
    });
    logic.on_typeahead(|items, current, search| {
        to_index(cl::typeahead(
            &choices(&items),
            usize::try_from(current).ok(),
            &search,
        ))
    });
    logic.on_list_metrics(|items, index, row, separator, padding| {
        let metrics = cl::list_metrics(
            &choices(&items),
            entry(index),
            row as f64,
            separator as f64,
            padding as f64,
        );
        ChoiceListMetrics {
            content_height: metrics.content_height as f32,
            entry_top: metrics.entry_top as f32,
        }
    });
    logic.on_font_select_choices(|recommended, recommended_label, system, system_label| {
        let label = |label: slint::SharedString| crate::Choice {
            kind: crate::ChoiceKind::Label,
            label,
            ..Default::default()
        };
        let separator = crate::Choice {
            kind: crate::ChoiceKind::Separator,
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
