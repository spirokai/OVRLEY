//! Keyboard navigation over a list of choices (tab triggers, toggle-group items, select items):
//! roving focus with optional wrapping, and type-ahead. Slint has no loops, so the components
//! ask for these results.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Item,
    /// A group label (`SelectLabel`).
    Label,
    Separator,
}

/// One entry of a choice list.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub kind: ChoiceKind,
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl Choice {
    pub fn item(value: &str, label: &str) -> Choice {
        Choice {
            kind: ChoiceKind::Item,
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }

    pub fn label(label: &str) -> Choice {
        Choice {
            kind: ChoiceKind::Label,
            value: String::new(),
            label: label.into(),
            disabled: false,
        }
    }

    pub fn separator() -> Choice {
        Choice {
            kind: ChoiceKind::Separator,
            value: String::new(),
            label: String::new(),
            disabled: false,
        }
    }

    /// Items can be focused and chosen; labels, separators and disabled items cannot.
    pub fn selectable(&self) -> bool {
        self.kind == ChoiceKind::Item && !self.disabled
    }
}

/// The index of the selectable choice whose value is `value`.
pub fn index_of(choices: &[Choice], value: &str) -> Option<usize> {
    choices
        .iter()
        .position(|choice| choice.selectable() && choice.value == value)
}

/// The first selectable choice from `from` (inclusive), stepping by `direction` (1 or -1). With
/// `wrap`, the search continues past the ends; without it, it stops there.
pub fn next_selectable(
    choices: &[Choice],
    from: isize,
    direction: isize,
    wrap: bool,
) -> Option<usize> {
    let len = choices.len() as isize;
    (0..len)
        .map(|step| from + step * direction)
        .map(|index| {
            if wrap {
                index.rem_euclid(len.max(1))
            } else {
                index
            }
        })
        .take_while(|index| (0..len).contains(index))
        .map(|index| index as usize)
        .find(|&index| choices[index].selectable())
}

/// Type-ahead: the next selectable choice whose label starts with `search`
/// (case-insensitive), searching from the current choice. A search of one repeated character
/// cycles through the choices starting with it. `None` keeps the current choice.
pub fn typeahead(choices: &[Choice], current: Option<usize>, search: &str) -> Option<usize> {
    let mut chars = search.chars();
    let first = chars.next()?;
    let repeated = chars.all(|c| c == first);
    let needle = if repeated {
        first.to_lowercase().collect::<String>()
    } else {
        search.to_lowercase()
    };
    let len = choices.len();
    let start = current.unwrap_or(0);
    let exclude_current = needle.chars().count() == 1;
    let found = (0..len)
        .map(|step| (start + step) % len)
        .filter(|&index| choices[index].selectable())
        .filter(|&index| !(exclude_current && Some(index) == current))
        .find(|&index| choices[index].label.to_lowercase().starts_with(&needle));
    found.filter(|&index| Some(index) != current)
}

/// The vertical extent of a list: its content height and where one entry starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListMetrics {
    pub content_height: f64,
    pub entry_top: f64,
}

/// Lays the entries out top to bottom: items and labels are `row_height` tall, separators
/// `separator_height`, inside `padding` above and below. `entry` is the entry to locate.
pub fn list_metrics(
    choices: &[Choice],
    entry: usize,
    row_height: f64,
    separator_height: f64,
    padding: f64,
) -> ListMetrics {
    let height = |choice: &Choice| {
        if choice.kind == ChoiceKind::Separator {
            separator_height
        } else {
            row_height
        }
    };
    let entry_top = padding
        + choices[..entry.min(choices.len())]
            .iter()
            .map(height)
            .sum::<f64>();
    let content_height = 2.0 * padding + choices.iter().map(height).sum::<f64>();
    ListMetrics {
        content_height,
        entry_top,
    }
}

/// `FontSelectField`'s list: the recommended group under its label, then, when there are system
/// fonts, a separator and the system group under its label. Generic over the entry type so the
/// UI can group its own entries.
pub fn font_select_choices<T: Clone>(
    recommended: &[T],
    recommended_label: T,
    system: &[T],
    system_label: T,
    separator: T,
) -> Vec<T> {
    let mut choices = vec![recommended_label];
    choices.extend_from_slice(recommended);
    if !system.is_empty() {
        choices.push(separator);
        choices.push(system_label);
        choices.extend_from_slice(system);
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices(entries: &[(&str, bool)]) -> Vec<Choice> {
        entries
            .iter()
            .map(|&(label, selectable)| Choice {
                disabled: !selectable,
                ..Choice::item(&label.to_lowercase(), label)
            })
            .collect()
    }

    #[test]
    fn measures_rows_and_separators() {
        let list = font_select_choices(
            &[Choice::item("a", "A"), Choice::item("b", "B")],
            Choice::label("Recommended"),
            &[Choice::item("c", "C")],
            Choice::label("System"),
            Choice::separator(),
        );
        let metrics = list_metrics(&list, 5, 28.0, 9.0, 4.0);
        assert_eq!(metrics.entry_top, 4.0 + 3.0 * 28.0 + 9.0 + 28.0);
        assert_eq!(metrics.content_height, 8.0 + 5.0 * 28.0 + 9.0);
    }

    #[test]
    fn groups_font_choices_and_drops_an_empty_system_group() {
        let recommended = [Choice::item("inter", "Inter")];
        let kinds = |list: Vec<Choice>| list.iter().map(|c| c.kind).collect::<Vec<_>>();
        assert_eq!(
            kinds(font_select_choices(
                &recommended,
                Choice::label("R"),
                &[Choice::item("arial", "Arial")],
                Choice::label("S"),
                Choice::separator()
            )),
            vec![
                ChoiceKind::Label,
                ChoiceKind::Item,
                ChoiceKind::Separator,
                ChoiceKind::Label,
                ChoiceKind::Item
            ]
        );
        assert_eq!(
            kinds(font_select_choices(
                &recommended,
                Choice::label("R"),
                &[],
                Choice::label("S"),
                Choice::separator()
            )),
            vec![ChoiceKind::Label, ChoiceKind::Item]
        );
    }

    #[test]
    fn finds_selectable_values() {
        let list = choices(&[("Group", false), ("Alpha", true), ("Beta", false)]);
        assert_eq!(index_of(&list, "alpha"), Some(1));
        assert_eq!(index_of(&list, "beta"), None);
        assert_eq!(index_of(&list, "missing"), None);
    }

    #[test]
    fn steps_over_unselectable_choices_with_and_without_wrapping() {
        let list = choices(&[("A", true), ("B", false), ("C", true), ("D", false)]);
        assert_eq!(next_selectable(&list, 1, 1, true), Some(2));
        assert_eq!(next_selectable(&list, 3, 1, true), Some(0));
        assert_eq!(next_selectable(&list, 3, 1, false), None);
        assert_eq!(next_selectable(&list, -1, -1, true), Some(2));
        assert_eq!(next_selectable(&list, 1, -1, false), Some(0));
        assert_eq!(next_selectable(&choices(&[("A", false)]), 0, 1, true), None);
    }

    #[test]
    fn typeahead_matches_label_prefixes_from_the_current_choice() {
        let list = choices(&[
            ("Arial", true),
            ("Bebas", true),
            ("Bold", true),
            ("Barlow", false),
            ("Calibri", true),
        ]);
        assert_eq!(typeahead(&list, Some(0), "be"), Some(1));
        assert_eq!(typeahead(&list, Some(0), "c"), Some(4));
        assert_eq!(typeahead(&list, Some(1), "b"), Some(2));
        assert_eq!(typeahead(&list, Some(2), "bb"), Some(1));
        assert_eq!(typeahead(&list, Some(1), "bebas"), None);
        assert_eq!(typeahead(&list, Some(0), "z"), None);
        assert_eq!(typeahead(&list, None, "BO"), Some(2));
    }
}
