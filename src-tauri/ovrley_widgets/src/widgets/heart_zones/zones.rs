//! Coggan's five heart-rate zones and where a sample falls on the equal-width track.
use crate::model::standard_metrics::heart_zone_model;
use serde::{Deserialize, Serialize};

pub const ZONE_COUNT: usize = 5;

/// The `heart_zones` display's `zoneModel`, shared with the editor's manual-zone seeding.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartZoneModel {
    /// Zone boundaries as shares of LTHR. Coggan's Z1 and Z5 are open-ended, so the
    /// manifest also fixes where the track starts and ends.
    pub lthr_boundaries: [f32; ZONE_COUNT + 1],
    /// Max-HR mode estimates LTHR as this share of maximum heart rate.
    pub lthr_share_of_max_heartrate: f32,
}

/// One zone's heart-rate span in bpm.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct HeartZoneRange {
    pub from: f32,
    pub to: f32,
}

pub fn coggan_zones(lthr: f32) -> [HeartZoneRange; ZONE_COUNT] {
    let boundaries = &heart_zone_model().lthr_boundaries;
    std::array::from_fn(|index| HeartZoneRange {
        from: lthr * boundaries[index],
        to: lthr * boundaries[index + 1],
    })
}

/// LTHR estimated from maximum heart rate.
pub fn lthr_from_max_heartrate(max_heartrate: f32) -> f32 {
    max_heartrate * heart_zone_model().lthr_share_of_max_heartrate
}

/// Active zone and the sample's 0..=1 progress across it. Samples between manual
/// zones belong to the lower one; samples outside the track clamp to its ends.
pub fn zone_position(zones: &[HeartZoneRange; ZONE_COUNT], heartrate: f32) -> (usize, f32) {
    let index = zones
        .iter()
        .rposition(|zone| heartrate >= zone.from)
        .unwrap_or(0);
    let zone = zones[index];
    (
        index,
        ((heartrate - zone.from) / (zone.to - zone.from)).clamp(0.0, 1.0),
    )
}
