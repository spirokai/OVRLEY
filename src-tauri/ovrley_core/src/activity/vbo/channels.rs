//! VBO channel-name normalization and canonical metric resolution.

use crate::error::{CoreError, CoreResult};
use std::collections::{BTreeMap, BTreeSet};

/// Device/source suffixes that are retained as channel qualifiers during normalization.
const DEVICE_QUALIFIERS: &[&str] = &[
    "acc", "baro", "canbus", "data", "gps", "gyro", "hrm", "lap", "logger", "magn", "obd", "wheel",
];
/// Qualifiers indicating calculated or estimated telemetry.
const OTHER_QUALIFIERS: &[&str] = &["calc", "calculated", "estimate", "estimated"];
/// Qualifiers identifying a physical position or numbered channel.
const POSITION_QUALIFIERS: &[&str] = &["front", "rear", "rr", "rl", "fr", "fl"];
/// Tokens recognized as measurement-unit suffixes and removed from metric bases.
const UNIT_TOKENS: &[&str] = &[
    "c", "deg", "degree", "degrees", "g", "hz", "kmh", "kph", "knots", "knot", "kts", "m", "meter",
    "meters", "metre", "metres", "mph", "pct", "percent", "s", "sec", "second", "seconds", "utc",
];

/// Units supported by VBO speed-like channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SpeedUnit {
    /// Kilometers per hour.
    KilometersPerHour,
    /// Meters per second.
    MetersPerSecond,
    /// Miles per hour.
    MilesPerHour,
    /// Nautical miles per hour.
    Knots,
}

impl SpeedUnit {
    /// Converts a value in this unit to meters per second.
    pub(super) fn to_meters_per_second(self, value: f64) -> f64 {
        match self {
            Self::KilometersPerHour => value / 3.6,
            Self::MetersPerSecond => value,
            Self::MilesPerHour => value * 0.447_04,
            Self::Knots => value * 0.514_444_444_444_444_5,
        }
    }
}

/// Normalized representation of one source channel from the VBO declarations.
#[derive(Clone, Debug)]
struct ChannelName {
    /// Original identifier from `[column names]` or the header fallback.
    identifier: String,
    /// One or more normalized metric bases from the identifier and optional header.
    bases: Vec<String>,
    /// Normalized source, calculation, unit, and position qualifiers.
    qualifiers: BTreeSet<String>,
    /// Original `[header]` text, when available for unit resolution and diagnostics.
    header: Option<String>,
}

/// Resolved source-column indexes and canonical units for supported VBO metrics.
#[derive(Clone, Debug)]
pub(super) struct ChannelLayout {
    /// Number of values required in every data row.
    pub(super) column_count: usize,
    /// Source index of the time-of-day channel.
    pub(super) time: Option<usize>,
    /// Source index of the Unix timestamp channel.
    pub(super) timestamp: Option<usize>,
    /// Source index of the relative elapsed-time channel.
    pub(super) elapsed_time: Option<usize>,
    /// Source index of latitude in VBO minutes.
    pub(super) latitude: Option<usize>,
    /// Source index of longitude in VBO minutes.
    pub(super) longitude: Option<usize>,
    /// Source index of speed.
    pub(super) speed: Option<usize>,
    /// Source index of heading.
    pub(super) heading: Option<usize>,
    /// Source index of elevation.
    pub(super) elevation: Option<usize>,
    /// Source index of vertical speed.
    pub(super) vertical_speed: Option<usize>,
    /// Source index of cumulative distance.
    pub(super) distance: Option<usize>,
    /// Source index of combined g-force.
    pub(super) g_force: Option<usize>,
    /// Source index of lateral or X-axis g-force.
    pub(super) g_force_x: Option<usize>,
    /// Source index of explicitly lateral acceleration for lean-angle derivation.
    pub(super) lateral_acceleration: Option<usize>,
    /// Source index of longitudinal or Y-axis g-force.
    pub(super) g_force_y: Option<usize>,
    /// Source index of vertical or Z-axis g-force.
    pub(super) g_force_z: Option<usize>,
    /// Source index of engine RPM.
    pub(super) rpm: Option<usize>,
    /// Source index of throttle position.
    pub(super) throttle_position: Option<usize>,
    /// Source index of brake position.
    pub(super) brake_position: Option<usize>,
    /// Source index of engine load.
    pub(super) engine_load: Option<usize>,
    /// Source index of lean angle.
    pub(super) lean_angle: Option<usize>,
    /// Source index of gear position.
    pub(super) gear_position: Option<usize>,
    /// Unit used to convert the selected speed channel.
    speed_unit: SpeedUnit,
    /// Unit used to convert the selected vertical-speed channel.
    vertical_speed_unit: SpeedUnit,
}

impl ChannelLayout {
    /// Converts a speed-channel value to meters per second.
    pub(super) fn speed_to_meters_per_second(&self, value: f64) -> f64 {
        self.speed_unit.to_meters_per_second(value)
    }

    /// Converts a vertical-speed-channel value to meters per second.
    pub(super) fn vertical_speed_to_meters_per_second(&self, value: f64) -> f64 {
        self.vertical_speed_unit.to_meters_per_second(value)
    }
}

/// Canonical telemetry metric that can be selected from a VBO channel declaration.
#[derive(Clone, Copy)]
enum Metric {
    /// Time of day encoded as `HHMMSS.ss`.
    Time,
    /// Unix timestamp in seconds.
    Timestamp,
    /// Relative elapsed time.
    ElapsedTime,
    /// Latitude in VBO minutes.
    Latitude,
    /// Longitude in VBO minutes.
    Longitude,
    /// Horizontal speed.
    Speed,
    /// Direction of travel.
    Heading,
    /// Elevation or altitude.
    Elevation,
    /// Vertical speed.
    VerticalSpeed,
    /// Cumulative traveled distance.
    Distance,
    /// Combined acceleration magnitude.
    GForce,
    /// Lateral or X-axis acceleration.
    GForceX,
    /// Explicitly lateral acceleration, excluding literal source X axes.
    LateralAcceleration,
    /// Longitudinal or Y-axis acceleration.
    GForceY,
    /// Vertical or Z-axis acceleration.
    GForceZ,
    /// Engine revolutions per minute.
    Rpm,
    /// Throttle or accelerator position.
    ThrottlePosition,
    /// Brake position.
    BrakePosition,
    /// Engine load as a percentage from zero to one hundred.
    EngineLoad,
    /// Vehicle lean angle.
    LeanAngle,
    /// Gear position.
    GearPosition,
}

impl Metric {
    /// Returns accepted normalized source-name aliases in priority order.
    fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Time => &["time", "utc_time"],
            Self::Timestamp => &["timestamp"],
            Self::ElapsedTime => &["elapsed_time"],
            Self::Latitude => &["lat", "latitude"],
            Self::Longitude => &["long", "lon", "longitude"],
            Self::Speed => &["velocity", "speed"],
            Self::Heading => &["heading", "bearing"],
            Self::Elevation => &["height", "altitude"],
            Self::VerticalSpeed => &["vertvel", "vert_vel", "vertical_velocity", "vertical_speed"],
            Self::Distance => &["distance", "distance_traveled", "distance_travelled"],
            Self::GForce => &["combined_acc", "g_force"],
            Self::GForceX => &[
                "lateral_acc",
                "lat_acc",
                "latacc",
                "lateral_accel",
                "lat_accel",
                "x_acc",
            ],
            Self::LateralAcceleration => &[
                "lateral_acc",
                "lat_acc",
                "latacc",
                "lateral_accel",
                "lat_accel",
            ],
            Self::GForceY => &[
                "longitudinal_acc",
                "long_acc",
                "longacc",
                "longitudinal_accel",
                "long_accel",
                "y_acc",
            ],
            Self::GForceZ => &["vertical_acc", "vert_acc", "vertacc", "z_acc"],
            Self::Rpm => &["rpm", "engine_rpm"],
            Self::ThrottlePosition => &[
                "accelerator_pos",
                "accelerator_position",
                "throttle_pos",
                "throttle_position",
            ],
            Self::BrakePosition => &["brake_pos", "brake_position"],
            Self::EngineLoad => &["engine_load", "engine load"],
            Self::LeanAngle => &["lean_angle"],
            Self::GearPosition => &["gear", "gear_position"],
        }
    }

    /// Ranks a channel by the preferred source for this metric.
    fn source_rank(self, channel: &ChannelName) -> usize {
        let has = |qualifier| channel.qualifiers.contains(qualifier);
        match self {
            Self::GForce
            | Self::GForceX
            | Self::LateralAcceleration
            | Self::GForceY
            | Self::GForceZ => {
                if has("canbus") {
                    0
                } else if has("acc") {
                    1
                } else if has("gps") {
                    2
                } else if has("calc") || has("calculated") {
                    3
                } else {
                    4
                }
            }
            Self::Rpm
            | Self::ThrottlePosition
            | Self::BrakePosition
            | Self::EngineLoad
            | Self::GearPosition => {
                if has("canbus") {
                    0
                } else if has("obd") {
                    1
                } else if has("calc") || has("calculated") {
                    3
                } else {
                    2
                }
            }
            _ => {
                if has("gps") {
                    0
                } else if has("calc") || has("calculated") {
                    2
                } else if has("canbus") || has("obd") {
                    3
                } else {
                    1
                }
            }
        }
    }

    /// Indicates whether source priority should be compared before alias priority.
    fn source_precedes_alias(self) -> bool {
        matches!(
            self,
            Self::GForce
                | Self::GForceX
                | Self::LateralAcceleration
                | Self::GForceY
                | Self::GForceZ
                | Self::Rpm
                | Self::ThrottlePosition
                | Self::BrakePosition
                | Self::EngineLoad
                | Self::GearPosition
        )
    }
}

/// Resolves VBO channel declarations into one unambiguous canonical layout.
///
/// The resolver validates declaration width, duplicate identifiers, required timeline and paired
/// coordinate channels, metric conflicts, supported telemetry presence, and declared speed units.
pub(super) fn resolve(header: &[String], identifiers: &[String]) -> CoreResult<ChannelLayout> {
    if !header.is_empty() && header.len() != identifiers.len() {
        return Err(CoreError::Activity(format!(
            "VBO header declares {} channels but the column layout declares {}",
            header.len(),
            identifiers.len()
        )));
    }

    let mut unique_identifiers = BTreeSet::new();
    for identifier in identifiers {
        if !unique_identifiers.insert(identifier.to_ascii_lowercase()) {
            return Err(CoreError::Activity(format!(
                "VBO column layout declares '{identifier}' more than once"
            )));
        }
    }

    let channels = identifiers
        .iter()
        .enumerate()
        .map(|(index, identifier)| channel_name(identifier, header.get(index)))
        .collect::<Vec<_>>();
    let time = resolve_metric(&channels, Metric::Time);
    let timestamp = resolve_metric(&channels, Metric::Timestamp);
    let elapsed_time = resolve_metric(&channels, Metric::ElapsedTime);
    if time.is_none() && timestamp.is_none() && elapsed_time.is_none() {
        return Err(CoreError::Activity(
            "VBO column layout has no time, timestamp, or elapsed_time column".to_string(),
        ));
    }
    let latitude = resolve_metric(&channels, Metric::Latitude);
    let longitude = resolve_metric(&channels, Metric::Longitude);
    if latitude.is_some() != longitude.is_some() {
        return Err(CoreError::Activity(
            "VBO column layout must declare latitude and longitude together".to_string(),
        ));
    }

    let speed = resolve_metric(&channels, Metric::Speed);
    let heading = resolve_metric(&channels, Metric::Heading);
    let elevation = resolve_metric(&channels, Metric::Elevation);
    let vertical_speed = resolve_metric(&channels, Metric::VerticalSpeed);
    let distance = resolve_metric(&channels, Metric::Distance);
    let g_force = resolve_metric(&channels, Metric::GForce);
    let g_force_x = resolve_metric(&channels, Metric::GForceX);
    let lateral_acceleration = resolve_metric(&channels, Metric::LateralAcceleration);
    let g_force_y = resolve_metric(&channels, Metric::GForceY);
    let g_force_z = resolve_metric(&channels, Metric::GForceZ);
    let rpm = resolve_metric(&channels, Metric::Rpm);
    let throttle_position = resolve_metric(&channels, Metric::ThrottlePosition);
    let brake_position = resolve_metric(&channels, Metric::BrakePosition);
    let engine_load = resolve_metric(&channels, Metric::EngineLoad);
    let lean_angle = resolve_metric(&channels, Metric::LeanAngle);
    let gear_position = resolve_metric(&channels, Metric::GearPosition);
    let mut metric_owners = BTreeMap::new();
    for (metric, index) in [
        ("time", time),
        ("timestamp", timestamp),
        ("elapsed_time", elapsed_time),
        ("latitude", latitude),
        ("longitude", longitude),
        ("speed", speed),
        ("heading", heading),
        ("elevation", elevation),
        ("vertical_speed", vertical_speed),
        ("distance", distance),
        ("g_force", g_force),
        ("g_force_x", g_force_x),
        ("g_force_y", g_force_y),
        ("g_force_z", g_force_z),
        ("rpm", rpm),
        ("throttle_position", throttle_position),
        ("brake_position", brake_position),
        ("engine_load", engine_load),
        ("lean_angle", lean_angle),
        ("gear_position", gear_position),
    ] {
        let Some(index) = index else {
            continue;
        };
        if let Some(existing) = metric_owners.insert(index, metric) {
            return Err(CoreError::Activity(format!(
                "VBO channel '{}' resolves to conflicting metrics {existing} and {metric}",
                identifiers[index]
            )));
        }
    }
    if [
        latitude,
        speed,
        heading,
        elevation,
        vertical_speed,
        distance,
        g_force,
        g_force_x,
        lateral_acceleration,
        g_force_y,
        g_force_z,
        rpm,
        throttle_position,
        brake_position,
        engine_load,
        lean_angle,
        gear_position,
    ]
    .iter()
    .all(Option::is_none)
    {
        return Err(CoreError::Activity(
            "VBO column layout has no supported telemetry columns".to_string(),
        ));
    }

    Ok(ChannelLayout {
        column_count: identifiers.len(),
        time,
        timestamp,
        elapsed_time,
        latitude,
        longitude,
        speed,
        heading,
        elevation,
        vertical_speed,
        distance,
        g_force,
        g_force_x,
        lateral_acceleration,
        g_force_y,
        g_force_z,
        rpm,
        throttle_position,
        brake_position,
        engine_load,
        lean_angle,
        gear_position,
        speed_unit: speed
            .map(|index| speed_unit(&channels[index], SpeedUnit::KilometersPerHour))
            .transpose()?
            .unwrap_or(SpeedUnit::KilometersPerHour),
        vertical_speed_unit: vertical_speed
            .map(|index| {
                let default = if channels[index]
                    .bases
                    .iter()
                    .any(|base| matches!(base.as_str(), "vertvel" | "vert_vel"))
                {
                    SpeedUnit::KilometersPerHour
                } else {
                    SpeedUnit::MetersPerSecond
                };
                speed_unit(&channels[index], default)
            })
            .transpose()?
            .unwrap_or(SpeedUnit::MetersPerSecond),
    })
}

/// Combines an identifier and optional header entry into one normalized channel description.
fn channel_name(identifier: &str, header: Option<&String>) -> ChannelName {
    let identifier_name = parse_name(identifier);
    let header_name = header.map(|value| parse_name(value));
    let mut bases = vec![identifier_name.0];
    let mut qualifiers = identifier_name.1;
    if let Some((base, header_qualifiers)) = header_name {
        if !bases.contains(&base) {
            bases.push(base);
        }
        qualifiers.extend(header_qualifiers);
    }
    ChannelName {
        identifier: identifier.to_string(),
        bases,
        qualifiers,
        header: header.cloned(),
    }
}

/// Normalizes a source channel name into a metric base and qualifier set.
///
/// RaceChrono prefixes, separator variants, unit suffixes, source qualifiers, and supported
/// numbered-position suffixes are handled here. Names containing unsupported punctuation remain
/// opaque instead of being repaired into a potentially different metric.
fn parse_name(raw: &str) -> (String, BTreeSet<String>) {
    if !raw.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '/' | ' ' | '\t')
    }) {
        return (raw.to_ascii_lowercase(), BTreeSet::new());
    }
    let lower = raw.to_ascii_lowercase();
    let without_prefix = lower.strip_prefix("rc_").unwrap_or(&lower);
    let normalized = normalize_separators(without_prefix);
    if matches!(normalized.as_str(), "event1" | "event_1") {
        return ("event".to_string(), BTreeSet::new());
    }
    let mut tokens = normalized
        .split('_')
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut qualifiers = BTreeSet::new();
    while tokens.len() > 1 && has_trailing_unit(&tokens) {
        if tokens.len() > 2
            && is_compound_unit_pair(
                tokens[tokens.len() - 2].as_str(),
                tokens[tokens.len() - 1].as_str(),
            )
        {
            tokens.truncate(tokens.len() - 2);
        } else {
            tokens.pop();
        }
    }
    if tokens.last().is_some_and(|last| {
        DEVICE_QUALIFIERS.contains(&last.as_str()) && (last != "acc" || tokens.len() > 2)
    }) {
        qualifiers.insert(tokens.pop().expect("device qualifier exists"));
    }
    if tokens
        .last()
        .is_some_and(|last| OTHER_QUALIFIERS.contains(&last.as_str()))
    {
        qualifiers.insert(tokens.pop().expect("calculation qualifier exists"));
    }
    if let Some(number) = tokens.last().and_then(|last| last.parse::<u8>().ok()) {
        if (1..=63).contains(&number) {
            tokens.pop();
            if tokens
                .last()
                .is_some_and(|last| POSITION_QUALIFIERS.contains(&last.as_str()))
            {
                tokens.pop();
            }
        }
    } else if tokens
        .last()
        .is_some_and(|last| POSITION_QUALIFIERS.contains(&last.as_str()))
    {
        tokens.pop();
    }
    if tokens.len() > 1 && tokens[0] == "gps" {
        qualifiers.insert(tokens.remove(0));
    }
    (tokens.join("_"), qualifiers)
}

/// Reports whether the token list ends in a recognized unit or compound unit.
fn has_trailing_unit(tokens: &[String]) -> bool {
    tokens
        .last()
        .is_some_and(|last| UNIT_TOKENS.contains(&last.as_str()))
        || (tokens.len() > 2
            && is_compound_unit_pair(
                tokens[tokens.len() - 2].as_str(),
                tokens[tokens.len() - 1].as_str(),
            ))
}

/// Lowercases a channel name and collapses non-alphanumeric runs into underscores.
fn normalize_separators(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut separator_pending = false;
    for character in raw.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !normalized.is_empty() {
                normalized.push('_');
            }
            normalized.push(character);
            separator_pending = false;
        } else {
            separator_pending = true;
        }
    }
    normalized
}

/// Reports whether two adjacent tokens form a supported compound unit such as `km/h`.
fn is_compound_unit_pair(left: &str, right: &str) -> bool {
    matches!(
        (left, right),
        ("km", "h") | ("km", "hr") | ("m", "s") | ("m", "sec")
    )
}

/// Selects the best source column for a metric using alias, source, and declaration order.
fn resolve_metric(channels: &[ChannelName], metric: Metric) -> Option<usize> {
    channels
        .iter()
        .enumerate()
        .filter_map(|(index, channel)| {
            let alias_rank = metric
                .aliases()
                .iter()
                .position(|alias| channel.bases.iter().any(|base| base == alias))?;
            let source_rank = metric.source_rank(channel);
            let score = if metric.source_precedes_alias() {
                (source_rank, alias_rank, index)
            } else {
                (alias_rank, source_rank, index)
            };
            Some((score, index))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, index)| index)
}

/// Resolves a channel's declared speed-like unit from its identifier and header.
///
/// Conflicting declarations are rejected; an absent declaration uses the metric-specific default.
fn speed_unit(channel: &ChannelName, default: SpeedUnit) -> CoreResult<SpeedUnit> {
    let identifier_unit = declared_speed_unit(&channel.identifier);
    let header_unit = channel.header.as_deref().and_then(declared_speed_unit);
    if identifier_unit.is_some() && header_unit.is_some() && identifier_unit != header_unit {
        return Err(CoreError::Activity(format!(
            "VBO channel '{}' declares conflicting units in [column names] and [header]",
            channel.identifier
        )));
    }
    Ok(identifier_unit.or(header_unit).unwrap_or(default))
}

/// Extracts a supported speed-like unit declaration from a raw channel name.
fn declared_speed_unit(name: &str) -> Option<SpeedUnit> {
    let normalized = normalize_separators(name);
    let tokens = normalized.split('_').collect::<Vec<_>>();
    if tokens.iter().any(|token| matches!(*token, "mph")) {
        Some(SpeedUnit::MilesPerHour)
    } else if tokens
        .iter()
        .any(|token| matches!(*token, "knot" | "knots" | "kts"))
    {
        Some(SpeedUnit::Knots)
    } else if tokens.iter().any(|token| matches!(*token, "kmh" | "kph"))
        || tokens
            .windows(2)
            .any(|pair| is_compound_unit_pair(pair[0], pair[1]) && pair[0] == "km")
    {
        Some(SpeedUnit::KilometersPerHour)
    } else if tokens
        .windows(2)
        .any(|pair| is_compound_unit_pair(pair[0], pair[1]) && pair[0] == "m")
        || tokens.iter().any(|token| *token == "mps")
    {
        Some(SpeedUnit::MetersPerSecond)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    /// Keeps RaceChrono suffix parsing in its documented order and range.
    #[test]
    fn racechrono_suffixes_follow_the_documented_order_and_range() {
        assert_eq!(parse_name("speed_front_1-lap").0, "speed");
        assert_eq!(parse_name("speed_63-canbus").0, "speed");
        assert_eq!(parse_name("speed_0-canbus").0, "speed_0");
        assert_eq!(parse_name("speed_64-canbus").0, "speed_64");
        assert_eq!(parse_name("speed_1_front").0, "speed_1");
    }

    /// Applies the RaceChrono prefix and historical event aliases exactly.
    #[test]
    fn racechrono_prefix_and_historical_event_aliases_are_exact() {
        assert_eq!(parse_name("rc_brake_pos").0, "brake_pos");
        assert_eq!(parse_name("rc-brake-pos").0, "rc_brake_pos");
        assert_eq!(parse_name("event1").0, "event");
        assert_eq!(parse_name("event-1").0, "event");
        assert_ne!(parse_name("speed!!!gps").0, "speed");
    }
}
