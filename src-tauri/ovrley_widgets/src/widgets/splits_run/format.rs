//! Split text and the per-frame content that keys presentation caches.
use super::{
    table::{SplitLength, SplitTable},
    validation::ValidatedSplitsRun,
    BAR_TICKS, PLACEHOLDER,
};

/// Unitless `M:SS` pace.
pub fn format_pace(pace_seconds: f64) -> String {
    let seconds = pace_seconds.round() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Header length such as `1 KM`, `0.5 MI` or `400 M`.
pub fn format_split_length(length: SplitLength) -> String {
    let distance = format!("{:.2}", length.distance);
    let distance = distance.trim_end_matches('0').trim_end_matches('.');
    format!("{distance} {}", length.unit.render_label())
}

/// Rounded text and quantized bar length are the presentation cache key.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitsContent {
    pub completed: usize,
    pub number: usize,
    pub pace_text: String,
    /// Running bar length in `1 / BAR_TICKS` of the full bar; absent while the running
    /// split has no pace, which draws the placeholder bar.
    pub bar_ticks: Option<u32>,
}

/// Without a split table (no activity, or its table still in flight) the first split shows a placeholder.
pub fn splits_content(table: Option<&SplitTable>, second: f64) -> SplitsContent {
    let Some(table) = table else {
        return SplitsContent {
            completed: 0,
            number: 1,
            pace_text: PLACEHOLDER.into(),
            bar_ticks: None,
        };
    };
    let sample = table.at(second);
    SplitsContent {
        completed: sample.completed,
        number: sample.number,
        pace_text: sample.pace_seconds.map_or_else(|| PLACEHOLDER.into(), format_pace),
        bar_ticks: sample
            .pace_seconds
            .map(|pace| (table.bar_fraction(pace) * f64::from(BAR_TICKS)).round() as u32),
    }
}

/// One drawn table row; the running split is always the first.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitRow {
    pub number: usize,
    pub pace_text: String,
    pub bar_fraction: f32,
}

/// Completed rows shown under the running split, newest first, within `max_splits` rows in total.
pub fn completed_rows(config: &ValidatedSplitsRun, table: Option<&SplitTable>, completed: usize) -> Vec<SplitRow> {
    let Some(table) = table else {
        return Vec::new();
    };
    table.completed[..completed]
        .iter()
        .enumerate()
        .rev()
        .take(config.max_splits - 1)
        .map(|(index, split)| SplitRow {
            number: table.first_split + index + 1,
            pace_text: format_pace(split.pace_seconds),
            bar_fraction: table.bar_fraction(split.pace_seconds) as f32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::splits_run::table::SplitUnit;

    #[test]
    fn pace_and_length_text() {
        assert_eq!(format_pace(299.6), "5:00");
        assert_eq!(format_pace(272.4), "4:32");
        assert_eq!(format_pace(3725.0), "62:05");
        let length = |distance, unit| format_split_length(SplitLength { distance, unit });
        assert_eq!(length(1.0, SplitUnit::Km), "1 KM");
        assert_eq!(length(0.5, SplitUnit::Mi), "0.5 MI");
        assert_eq!(length(400.0, SplitUnit::M), "400 M");
        assert_eq!(length(1.25, SplitUnit::Km), "1.25 KM");
    }
}
