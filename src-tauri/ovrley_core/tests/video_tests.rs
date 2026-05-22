use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::video::composite_output_frame_windows;

#[test]
fn composite_output_frame_windows_share_exact_frame_boundary_times() {
    let fps = Fps::new(30000, 1001).unwrap();
    let windows = composite_output_frame_windows(150, 5.0, fps, 2);
    let split_time = 75.0 / fps.as_f64();

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].output_start_frame, 0);
    assert_eq!(windows[0].output_end_frame, 75);
    assert_eq!(windows[1].output_start_frame, 75);
    assert_eq!(windows[1].output_end_frame, 150);
    assert!(windows[0].video_start_seconds.abs() <= 1e-12);
    assert!((windows[0].render_duration_seconds - split_time).abs() <= 1e-9);
    assert!((windows[1].video_start_seconds - split_time).abs() <= 1e-9);
    assert!((windows[1].render_duration_seconds - (5.0 - split_time)).abs() <= 1e-9);
}

#[test]
fn composite_output_frame_windows_keep_fractional_tail_on_last_segment() {
    let fps = Fps::new(30000, 1001).unwrap();
    let windows = composite_output_frame_windows(4, 0.101, fps, 2);

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].output_start_frame, 0);
    assert_eq!(windows[0].output_end_frame, 2);
    assert_eq!(windows[1].output_start_frame, 2);
    assert_eq!(windows[1].output_end_frame, 4);
    assert!((windows[0].render_duration_seconds - (2.0 / fps.as_f64())).abs() <= 1e-9);
    assert!((windows[1].video_start_seconds - (2.0 / fps.as_f64())).abs() <= 1e-9);
    assert!((windows[1].render_duration_seconds - (0.101 - (2.0 / fps.as_f64()))).abs() <= 1e-9);
}
