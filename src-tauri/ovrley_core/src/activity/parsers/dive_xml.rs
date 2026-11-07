//! Shared helpers at the native diving XML boundary.

use super::helpers::{self, finite_number};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta};
use roxmltree::Node;

/// Finds child elements in the parent's namespace.
pub(super) fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    helpers::children(node, name, node.tag_name().namespace())
}

pub(super) fn child_text<'a>(node: Node<'a, '_>, name: &'a str) -> Option<&'a str> {
    helpers::child_text(node, name, node.tag_name().namespace())
}

/// Reads a number, accepting a decimal comma.
pub(super) fn number(text: &str) -> Option<f64> {
    finite_number(&text.trim().replace(',', "."))
}

pub(super) fn single_dive<'a, 'input>(
    root: Node<'a, 'input>,
    format: &str,
) -> CoreResult<Node<'a, 'input>> {
    let mut dives = root.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "dive"
            && node.tag_name().namespace() == root.tag_name().namespace()
            // UDDF can include header-only previous-dive references.
            && (children(*node, "samples").next().is_some()
                || children(*node, "divecomputer").next().is_some())
    });
    let dive = dives.next().ok_or_else(|| {
        CoreError::Activity(format!("The {format} file does not contain any dives"))
    })?;
    if dives.next().is_some() {
        return Err(CoreError::Activity(format!(
            "The {format} file contains multiple dives; export a file containing a single dive"
        )));
    }
    Ok(dive)
}

pub(super) fn start_time(
    text: Option<&str>,
    format: &str,
) -> CoreResult<Option<DateTime<FixedOffset>>> {
    text.map(|text| {
        DateTime::parse_from_rfc3339(text.trim())
            .or_else(|_| {
                // These exports can omit the offset. With no GPS timeline, retain
                // the shared finalizer's convention of interpreting naive time as UTC.
                NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S%.f")
                    .map(|time| time.and_utc().fixed_offset())
            })
            .map_err(|error| {
                CoreError::Activity(format!("Invalid {format} dive start time: {error}"))
            })
    })
    .transpose()
}

pub(super) fn timestamp(start: Option<DateTime<FixedOffset>>, elapsed: f64) -> Option<String> {
    start?
        .checked_add_signed(TimeDelta::try_milliseconds(
            (elapsed * 1000.0).round() as i64
        )?)
        .map(|time| time.to_rfc3339())
}

pub(super) fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / count as f64)
}

/// A gas observation is atomic; an omitted constituent is not zero helium.
pub(super) fn gas_fractions(
    oxygen: Option<f64>,
    helium: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    match (oxygen, helium) {
        (Some(oxygen), Some(helium))
            if (0.0..=1.0).contains(&oxygen)
                && (0.0..=1.0).contains(&helium)
                && oxygen + helium <= 1.0 =>
        {
            (Some(oxygen), Some(helium))
        }
        _ => (None, None),
    }
}
