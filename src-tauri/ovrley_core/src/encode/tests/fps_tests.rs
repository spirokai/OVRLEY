use super::Fps;

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
