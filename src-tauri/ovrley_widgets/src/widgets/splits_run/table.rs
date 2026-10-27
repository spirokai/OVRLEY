//! Split table contract. `ovrley_core` derives it from the activity; widgets only look values up.
use crate::error::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};

const METERS_PER_KILOMETER: f64 = 1000.0;
const METERS_PER_MILE: f64 = 1609.344;
const METERS_PER_FOOT: f64 = 0.3048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitUnit {
    M,
    Km,
    Ft,
    Mi,
}

impl SplitUnit {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "m" => Some(Self::M),
            "km" => Some(Self::Km),
            "ft" => Some(Self::Ft),
            "mi" => Some(Self::Mi),
            _ => None,
        }
    }

    pub fn meters(self) -> f64 {
        match self {
            Self::M => 1.0,
            Self::Km => METERS_PER_KILOMETER,
            Self::Ft => METERS_PER_FOOT,
            Self::Mi => METERS_PER_MILE,
        }
    }

    /// Metric splits report time per kilometer, imperial splits time per mile.
    pub fn pace_meters(self) -> f64 {
        match self {
            Self::M | Self::Km => METERS_PER_KILOMETER,
            Self::Ft | Self::Mi => METERS_PER_MILE,
        }
    }

    pub fn render_label(self) -> &'static str {
        match self {
            Self::M => "M",
            Self::Km => "KM",
            Self::Ft => "FT",
            Self::Mi => "MI",
        }
    }
}

/// The configured split length; tables are looked up by exact equality.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitLength {
    pub distance: f64,
    pub unit: SplitUnit,
}

impl SplitLength {
    pub fn meters(self) -> f64 {
        self.distance * self.unit.meters()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletedSplit {
    /// Activity second at which the split's end boundary was crossed.
    pub end_seconds: f64,
    /// Seconds per pace unit ([`SplitUnit::pace_meters`]).
    pub pace_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitTable {
    pub length: SplitLength,
    /// 0-based index of the first split the distance series observes.
    pub first_split: usize,
    /// Completed splits in order.
    pub completed: Vec<CompletedSplit>,
    /// Original-density seconds of the running split's average pace.
    pub live_seconds: Vec<f64>,
    /// Running split's average pace, smoothed within each split; none before it covers distance.
    pub live_pace_seconds: Vec<Option<f64>>,
    /// The pace drawn as a full-length bar.
    pub slowest_pace_seconds: Option<f64>,
}

/// The split running at an activity second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitSample {
    pub completed: usize,
    /// 1-based number of the running split.
    pub number: usize,
    pub pace_seconds: Option<f64>,
}

impl SplitTable {
    pub fn from_json(bytes: &[u8]) -> WidgetResult<Self> {
        let table: Self = serde_json::from_slice(bytes)
            .map_err(|error| WidgetError::Config(format!("split table: {error}")))?;
        table.validate()?;
        Ok(table)
    }

    fn validate(&self) -> WidgetResult<()> {
        let invalid = |message: &str| Err(WidgetError::Config(format!("split table: {message}")));
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !positive(self.length.distance) {
            return invalid("length must be finite and positive");
        }
        if self
            .completed
            .iter()
            .any(|split| !split.end_seconds.is_finite() || !positive(split.pace_seconds))
            || self
                .completed
                .windows(2)
                .any(|pair| pair[0].end_seconds >= pair[1].end_seconds)
        {
            return invalid("completed splits must be ordered with finite positive paces");
        }
        if self.live_seconds.len() != self.live_pace_seconds.len()
            || self.live_seconds.iter().any(|second| !second.is_finite())
            || self.live_seconds.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .live_pace_seconds
                .iter()
                .flatten()
                .any(|pace| !positive(*pace))
        {
            return invalid("live paces must align with increasing finite seconds");
        }
        let has_pace =
            !self.completed.is_empty() || self.live_pace_seconds.iter().any(Option::is_some);
        match self.slowest_pace_seconds {
            Some(pace) if has_pace && positive(pace) => Ok(()),
            None if !has_pace => Ok(()),
            _ => invalid("slowest pace must be positive exactly when a pace exists"),
        }
    }

    fn completed_before(&self, second: f64) -> usize {
        self.completed
            .partition_point(|split| split.end_seconds <= second + 1e-9)
    }

    pub fn at(&self, second: f64) -> SplitSample {
        let completed = self.completed_before(second);
        SplitSample {
            completed,
            number: self.first_split + completed + 1,
            pace_seconds: self.live_pace_at(completed, second),
        }
    }

    /// Interpolates between the running split's own samples; the edges hold its nearest one.
    fn live_pace_at(&self, completed: usize, second: f64) -> Option<f64> {
        if self
            .live_seconds
            .first()
            .is_none_or(|first| second < *first)
        {
            return None;
        }
        let next = self.live_seconds.partition_point(|time| *time <= second);
        let sample = |index: usize| {
            self.live_seconds.get(index).and_then(|time| {
                (self.completed_before(*time) == completed)
                    .then(|| self.live_pace_seconds[index].map(|pace| (*time, pace)))
                    .flatten()
            })
        };
        match (next.checked_sub(1).and_then(sample), sample(next)) {
            (Some((t0, p0)), Some((t1, p1))) => Some(p0 + (p1 - p0) * (second - t0) / (t1 - t0)),
            (Some((_, pace)), None) | (None, Some((_, pace))) => Some(pace),
            (None, None) => None,
        }
    }

    /// Bar share of the full bar length; paces beyond the slowest clamp to full.
    pub fn bar_fraction(&self, pace_seconds: f64) -> f64 {
        self.slowest_pace_seconds
            .map_or(0.0, |slowest| (pace_seconds / slowest).min(1.0))
    }

    /// Slowest pace any row can print, for stable column widths.
    pub fn slowest_printed_pace(&self) -> Option<f64> {
        self.completed
            .iter()
            .map(|split| split.pace_seconds)
            .chain(self.live_pace_seconds.iter().flatten().copied())
            .reduce(f64::max)
    }

    /// Highest split number any row can print.
    pub fn last_number(&self) -> usize {
        self.first_split + self.completed.len() + 1
    }
}
