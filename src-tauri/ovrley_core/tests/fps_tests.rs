use ovrley_core::encode::fps::Fps;

#[test]
// Verifies 29.97-style rates remain exact rational values.
fn preserves_ntsc_rational_fps() {
    let fps = Fps::new(30000, 1001).unwrap();

    assert!((fps.as_f64() - 29.97002997).abs() < 0.00000001);
    assert_eq!(fps.ffmpeg_arg(), "30000/1001");
}

#[test]
// Verifies overlay pipe FPS is exactly derived by integer update factors.
fn divides_overlay_fps_exactly() {
    let source = Fps::new(60000, 1001).unwrap();

    assert_eq!(
        source.divided_by(2).unwrap(),
        Fps::new(30000, 1001).unwrap()
    );
    assert_eq!(
        source.divided_by(6).unwrap(),
        Fps::new(10000, 1001).unwrap()
    );
}

#[test]
// Verifies zero factors are rejected before division.
fn rejects_zero_division_factor() {
    let source = Fps::new(60000, 1001).unwrap();

    assert_eq!(
        source.divided_by(0).unwrap_err(),
        "FPS division factor must be greater than zero"
    );
}

#[test]
// Verifies common float metadata can be converted when rationals are absent.
fn converts_common_float_fallback_rates() {
    assert_eq!(
        Fps::from_f64_fallback(23.976).unwrap().ffmpeg_arg(),
        "24000/1001"
    );
    assert_eq!(
        Fps::from_f64_fallback(29.97).unwrap().ffmpeg_arg(),
        "30000/1001"
    );
    assert_eq!(
        Fps::from_f64_fallback(59.94).unwrap().ffmpeg_arg(),
        "60000/1001"
    );
    assert_eq!(Fps::from_f64_fallback(25.0).unwrap().ffmpeg_arg(), "25/1");
    assert_eq!(Fps::from_f64_fallback(30.0).unwrap().ffmpeg_arg(), "30/1");
    assert_eq!(Fps::from_f64_fallback(60.0).unwrap().ffmpeg_arg(), "60/1");
}

// --- Snapshot / golden tests (Step 11d) ---

#[test]
fn integer_fps_frame_count_for_duration() {
    let fps = Fps::new(30, 1).unwrap();
    assert!((fps.as_f64() - 30.0).abs() < 1e-12);
    assert_eq!((10.0 * fps.as_f64()).ceil() as u64, 300);
}

#[test]
fn ntsc_fps_precision_and_rounding() {
    let fps_2997 = Fps::new(30000, 1001).unwrap();
    let fps_5994 = Fps::new(60000, 1001).unwrap();

    assert!((fps_2997.as_f64() - 29.97002997).abs() < 1e-7);
    assert!((fps_5994.as_f64() - 59.94005994).abs() < 1e-7);
    assert_eq!(fps_2997.ffmpeg_arg(), "30000/1001");
    assert_eq!(fps_5994.ffmpeg_arg(), "60000/1001");
}

#[test]
fn fps_division_halves_and_quarters() {
    let fps60 = Fps::new(60, 1).unwrap();
    assert_eq!(fps60.divided_by(2).unwrap(), Fps::new(30, 1).unwrap());
    assert_eq!(fps60.divided_by(4).unwrap(), Fps::new(15, 1).unwrap());

    let fps5994 = Fps::new(60000, 1001).unwrap();
    assert_eq!(fps5994.divided_by(2).unwrap(), Fps::new(30000, 1001).unwrap());
}

#[test]
fn zero_denominator_rejected() {
    assert!(Fps::new(30, 0).is_err());
}

#[test]
fn fps_equality_and_comparison() {
    let a = Fps::new(30000, 1001).unwrap();
    let b = Fps::new(30000, 1001).unwrap();
    let c = Fps::new(30, 1).unwrap();

    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn fractional_duration_frame_count() {
    let fps = Fps::new(30, 1).unwrap();
    assert_eq!((7.3 * fps.as_f64()).ceil() as u64, 219);
    assert_eq!((0.033 * fps.as_f64()).ceil() as u64, 1);
}
