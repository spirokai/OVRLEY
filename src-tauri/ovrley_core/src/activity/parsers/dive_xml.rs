//! Shared helpers at the native diving XML boundary.

use crate::activity::finalize::{finalize_raw_activity, FinalizeActivityResponse};
use crate::activity::schema::RawActivity;
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta};
use roxmltree::Node;
use std::{fs::File, io::Read, path::Path};

pub(super) fn parse_path(
    path: &Path,
    repo_root: Option<&Path>,
    format: &str,
    extract: fn(&str, &str) -> CoreResult<RawActivity>,
) -> CoreResult<FinalizeActivityResponse> {
    let file = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CoreError::Activity(format!(
                "{format} path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })?;
    parse_reader(file, file_name, repo_root, format, extract)
}

pub(super) fn parse_reader<R: Read>(
    mut reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
    format: &str,
    extract: fn(&str, &str) -> CoreResult<RawActivity>,
) -> CoreResult<FinalizeActivityResponse> {
    let result = (|| {
        let mut text = String::new();
        reader.read_to_string(&mut text).map_err(|error| {
            CoreError::Activity(format!("Failed to read {format} input: {error}"))
        })?;
        finalize_raw_activity(&extract(&text, file_name)?, repo_root)
    })();
    result.map_err(|error| CoreError::Activity(format!("{format} import '{file_name}': {error}")))
}

pub(super) fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |child| {
        child.is_element()
            && child.tag_name().name() == name
            && child.tag_name().namespace() == node.tag_name().namespace()
    })
}

pub(super) fn child_text<'a>(node: Node<'a, '_>, name: &'a str) -> Option<&'a str> {
    children(node, name)
        .next()?
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

pub(super) fn number(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
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
