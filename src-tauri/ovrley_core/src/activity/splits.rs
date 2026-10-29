//! Running splits derived on demand for one split length.
//!
//! The split length is widget configuration, so finalization cannot precompute splits:
//! export derives a table per configured length and the editor requests them by activity id.
//! Splits count from the activity's distance origin, not the scene trim.

use super::finalize::smoothing::{smoothing_window_for_seconds, zero_phase_smooth};
use super::schema::ParsedActivity;
use ovrley_widgets::widgets::splits_run::table::{CompletedSplit, SplitLength, SplitTable};

/// Zero-phase smoothing horizon of the running split's average pace.
const LIVE_PACE_SMOOTHING_SECONDS: f64 = 5.0;

/// Splits of `length` over the activity's original-density distance.
pub fn derive_split_table(activity: &ParsedActivity, length: SplitLength) -> SplitTable {
    derive_splits(&activity.sample_elapsed_seconds, &activity.distance, length)
}

fn derive_splits(elapsed: &[f64], distance: &[Option<f64>], length: SplitLength) -> SplitTable {
    let split_meters = length.meters();
    let pace_meters = length.unit.pace_meters();
    // External distance may dip; splits follow the furthest distance reached.
    let points: Vec<(f64, f64)> = elapsed
        .iter()
        .zip(distance)
        .filter_map(|(second, meters)| meters.filter(|meters| meters.is_finite()).map(|meters| (*second, meters)))
        .scan(f64::NEG_INFINITY, |furthest, (second, meters)| {
            *furthest = furthest.max(meters);
            Some((second, *furthest))
        })
        .collect();
    let Some(&first) = points.first() else {
        return SplitTable {
            length,
            first_split: 0,
            completed: Vec::new(),
            live_seconds: Vec::new(),
            live_pace_seconds: Vec::new(),
            slowest_pace_seconds: None,
        };
    };

    let first_split = (first.1.max(0.0) / split_meters).floor() as usize;
    let mut split = first_split;
    let mut start = first;
    let mut previous = first;
    let mut completed = Vec::new();
    let mut live_seconds = Vec::with_capacity(points.len());
    let mut live_pace = Vec::with_capacity(points.len());
    let mut segment_starts = vec![0];
    for &(second, meters) in &points {
        loop {
            let boundary = (split + 1) as f64 * split_meters;
            if meters < boundary {
                break;
            }
            // Furthest distance only grows, so a crossing step always covers distance.
            let crossing = previous.0 + (boundary - previous.1) / (meters - previous.1) * (second - previous.0);
            completed.push(CompletedSplit {
                end_seconds: crossing,
                pace_seconds: (crossing - start.0) / (boundary - start.1) * pace_meters,
            });
            start = (crossing, boundary);
            split += 1;
            segment_starts.push(live_seconds.len());
        }
        live_seconds.push(second);
        live_pace.push((meters > start.1).then(|| (second - start.0) / (meters - start.1) * pace_meters));
        previous = (second, meters);
    }

    // Smoothing stays within each split so a new split never blends with the previous one.
    let timestamps_ms: Vec<f64> = live_seconds.iter().map(|second| second * 1000.0).collect();
    let window = smoothing_window_for_seconds(&timestamps_ms, LIVE_PACE_SMOOTHING_SECONDS);
    segment_starts.push(live_pace.len());
    let live_pace_seconds: Vec<Option<f64>> = segment_starts
        .windows(2)
        .flat_map(|range| zero_phase_smooth(&live_pace[range[0]..range[1]], window))
        .collect();

    // The full bar is the slowest split: every completed split and the unfinished last one at
    // its final sample. Transient running paces slower than that clamp to the full bar.
    let slowest_pace_seconds = completed
        .iter()
        .map(|split| split.pace_seconds)
        .chain(live_pace_seconds.last().copied().flatten())
        .reduce(f64::max);
    SplitTable {
        length,
        first_split,
        completed,
        live_seconds,
        live_pace_seconds,
        slowest_pace_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_widgets::widgets::splits_run::table::SplitUnit;

    const KILOMETER: SplitLength = SplitLength { distance: 1.0, unit: SplitUnit::Km };

    #[test]
    fn steady_pace_completes_exact_splits_and_keeps_the_live_average() {
        // 4 m/s for 2500 s: 250 s/km, boundaries at 250 s and 500 s.
        let elapsed: Vec<f64> = (0..=2500).map(f64::from).collect();
        let distance: Vec<_> = elapsed.iter().map(|second| Some(second * 4.0)).collect();
        let table = derive_splits(&elapsed, &distance, KILOMETER);
        assert_eq!(table.first_split, 0);
        assert_eq!(table.completed.len(), 10);
        for (index, split) in table.completed.iter().enumerate() {
            assert!((split.end_seconds - 250.0 * (index + 1) as f64).abs() < 1e-9);
            assert!((split.pace_seconds - 250.0).abs() < 1e-9);
        }
        assert_eq!(table.live_seconds.len(), elapsed.len());
        // Boundary samples have covered no distance in their new split yet.
        assert_eq!(table.live_pace_seconds[250], None);
        assert!(table.live_pace_seconds.iter().flatten().all(|pace| (pace - 250.0).abs() < 1e-6));
        assert_eq!(table.slowest_pace_seconds, Some(250.0));
    }

    #[test]
    fn imperial_splits_report_minutes_per_mile_and_gaps_interpolate_crossings() {
        // 1609.344 m per mile split at 5 m/s, with the crossing sample missing.
        let elapsed = vec![0.0, 300.0, 330.0, 400.0];
        let distance = vec![Some(0.0), Some(1500.0), None, Some(2000.0)];
        let mile = SplitLength { distance: 1.0, unit: SplitUnit::Mi };
        let table = derive_splits(&elapsed, &distance, mile);
        assert_eq!(table.completed.len(), 1);
        let crossing = 300.0 + (1609.344 - 1500.0) / 500.0 * 100.0;
        assert!((table.completed[0].end_seconds - crossing).abs() < 1e-9);
        assert!((table.completed[0].pace_seconds - crossing).abs() < 1e-9);
        assert_eq!(table.live_seconds, vec![0.0, 300.0, 400.0]);
    }

    #[test]
    fn later_distance_origin_and_dips_follow_the_furthest_distance() {
        // A stop one meter past the boundary spikes the live pace; the slow unfinished split ends at 900 s.
        let elapsed = vec![0.0, 100.0, 200.0, 300.0, 360.0, 900.0];
        let distance = vec![Some(1200.0), Some(1700.0), Some(1600.0), Some(2001.0), Some(2001.0), Some(2600.0)];
        let table = derive_splits(&elapsed, &distance, KILOMETER);
        assert_eq!(table.first_split, 1);
        assert_eq!(table.completed.len(), 1);
        // The first observed split covers 800 m; the dip holds 1700 m until distance resumes.
        let crossing = 200.0 + 300.0 / 301.0 * 100.0;
        assert!((table.completed[0].end_seconds - crossing).abs() < 1e-9);
        assert!((table.completed[0].pace_seconds - crossing / 800.0 * 1000.0).abs() < 1e-9);
        // The full bar is the slowest split, here the unfinished one at its final sample, not the spike.
        let final_pace = table.live_pace_seconds.last().copied().flatten().unwrap();
        assert!(final_pace > table.completed[0].pace_seconds);
        assert_eq!(table.slowest_pace_seconds, Some(final_pace));
        assert!(table.live_pace_seconds.iter().flatten().any(|pace| *pace > final_pace * 2.0), "the stop spikes the live pace");
    }

    #[test]
    fn activity_without_distance_has_an_empty_table() {
        let table = derive_splits(&[0.0, 1.0], &[], KILOMETER);
        assert!(table.completed.is_empty() && table.live_seconds.is_empty());
        assert_eq!(table.slowest_pace_seconds, None);
    }
}
