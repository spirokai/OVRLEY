use super::layout::{normalized, sample_times, wrap_entries};

#[test]
fn samples_span_both_window_edges_on_a_fixed_time_grid() {
    let times = sample_times(12.25, 10.0, 0.5);
    assert_eq!(times.first(), Some(&2.25));
    assert_eq!(times.last(), Some(&12.25));
    assert_eq!(times[1..times.len() - 1], (5..=24).map(|k| k as f64 * 0.5).collect::<Vec<_>>());
    // Grid points never repeat an edge that falls on the grid.
    let aligned = sample_times(12.0, 10.0, 0.5);
    assert_eq!(aligned.len(), 21);
    assert!(aligned.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn grid_samples_keep_their_time_as_the_window_slides() {
    let earlier = sample_times(20.0, 5.0, 0.25);
    let later = sample_times(20.1, 5.0, 0.25);
    for time in [15.25, 17.5, 19.75] {
        assert!(earlier.contains(&time) && later.contains(&time));
    }
}

#[test]
fn values_scale_to_their_activity_range_and_flat_metrics_sit_mid_height() {
    assert_eq!(normalized(100.0, (100.0, 200.0)), 0.0);
    assert_eq!(normalized(200.0, (100.0, 200.0)), 1.0);
    assert_eq!(normalized(125.0, (100.0, 200.0)), 0.25);
    assert_eq!(normalized(42.0, (42.0, 42.0)), 0.5);
}

#[test]
fn legend_entries_wrap_onto_new_rows_at_the_plot_width() {
    // Gap 10, width 100: 40 + 10 + 40 fits; the third entry starts a new row.
    assert_eq!(wrap_entries(&[40.0, 40.0, 30.0], 10.0, 100.0), vec![(0, 0.0), (0, 50.0), (1, 0.0)]);
    // An entry wider than the plot still occupies its own row instead of an empty one.
    assert_eq!(wrap_entries(&[150.0, 20.0], 10.0, 100.0), vec![(0, 0.0), (1, 0.0)]);
    assert_eq!(wrap_entries(&[20.0, 150.0], 10.0, 100.0), vec![(0, 0.0), (1, 0.0)]);
}
