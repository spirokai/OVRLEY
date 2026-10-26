use super::{
    format::splits_content,
    table::{CompletedSplit, SplitLength, SplitTable, SplitUnit},
    BAR_TICKS,
};

fn table() -> SplitTable {
    SplitTable {
        length: SplitLength { distance: 1.0, unit: SplitUnit::Km },
        first_split: 0,
        completed: vec![
            CompletedSplit { end_seconds: 300.0, pace_seconds: 300.0 },
            CompletedSplit { end_seconds: 540.0, pace_seconds: 240.0 },
        ],
        live_seconds: vec![0.0, 150.0, 299.0, 301.0, 400.0, 541.0, 600.0],
        live_pace_seconds: vec![None, Some(310.0), Some(300.0), Some(250.0), Some(245.0), Some(280.0), Some(270.0)],
        slowest_pace_seconds: Some(300.0),
    }
}

#[test]
fn running_split_interpolates_its_own_samples_and_holds_at_edges() {
    let table = table();
    let sample = table.at(75.0);
    assert_eq!((sample.completed, sample.number), (0, 1));
    assert_eq!(sample.pace_seconds, Some(310.0));
    assert_eq!(table.at(299.5).pace_seconds, Some(300.0));
    // Just past a boundary, the next split holds its first sample instead of blending.
    let next = table.at(300.5);
    assert_eq!((next.completed, next.number), (1, 2));
    assert_eq!(next.pace_seconds, Some(250.0));
    assert!((table.at(350.5).pace_seconds.unwrap() - 247.5).abs() < 1e-9);
    // After the last sample the final split holds its last value.
    assert_eq!(table.at(900.0).pace_seconds, Some(270.0));
    assert_eq!(table.at(-1.0).pace_seconds, None);
}

#[test]
fn content_quantizes_bars_against_the_slowest_split() {
    let table = table();
    let content = splits_content(Some(&table), 450.0);
    assert_eq!(content.completed, 1);
    // The next sample belongs to split 3, so split 2 holds its last one.
    assert_eq!(content.pace_text, "4:05");
    assert_eq!(content.bar_ticks, Some((245.0 / 300.0 * f64::from(BAR_TICKS)).round() as u32));
    let missing = splits_content(None, 450.0);
    assert_eq!((missing.number, missing.pace_text.as_str(), missing.bar_ticks), (1, "-:--", None));
    // Before the running split covers distance it shows the same placeholder.
    assert_eq!(splits_content(Some(&table), -1.0).bar_ticks, None);
}

#[test]
fn malformed_tables_fail_at_ingress() {
    let valid = serde_json::to_vec(&table()).unwrap();
    assert!(SplitTable::from_json(&valid).is_ok());
    let mut unordered = table();
    unordered.completed.swap(0, 1);
    assert!(SplitTable::from_json(&serde_json::to_vec(&unordered).unwrap()).is_err());
    let mut misaligned = table();
    misaligned.live_pace_seconds.pop();
    assert!(SplitTable::from_json(&serde_json::to_vec(&misaligned).unwrap()).is_err());
    let mut unscaled = table();
    unscaled.slowest_pace_seconds = None;
    assert!(SplitTable::from_json(&serde_json::to_vec(&unscaled).unwrap()).is_err());
}
