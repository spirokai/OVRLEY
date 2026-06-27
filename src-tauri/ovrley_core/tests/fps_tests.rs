//! Rational FPS type tests.
//!
//! Verifies `Fps` arithmetic: construction, float fallback for common
//! NTSC rates (23.976, 29.97, 59.94), common integer rates, division for overlay pipe FPS
//! derivation, ffmpeg argument formatting, equality/comparison, and
//! rejection of invalid values (zero denominator, zero division factor).
//!
//! ## Type
//! Unit test. Pure math — no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - NTSC rates rounding to integer FPS in ffmpeg args
//! - Zero denominators or division factors accepted
//! - Overlay FPS division producing unrepresentable rationals
//! - Float fallback missing a common NTSC rate

use ovrley_core::encode::fps::Fps;

#[test]
// Verifies common NTSC-rate rational pairs preserve precision.
fn preserves_ntsc_rational_fps() {
    let fps_2997 = Fps::new(30000, 1001).unwrap();
    let fps_5994 = Fps::new(60000, 1001).unwrap();

    assert!((fps_2997.as_f64() - 29.97002997).abs() < 0.00000001);
    assert_eq!(fps_2997.ffmpeg_arg(), "30000/1001");

    assert!((fps_5994.as_f64() - 59.94005994).abs() < 0.00000001);
    assert_eq!(fps_5994.ffmpeg_arg(), "60000/1001");
}

#[test]
// Verifies overlay pipe FPS is exactly derived by integer update factors.
fn divides_overlay_fps_exactly() {
    // Rational divisions
    let ntsc = Fps::new(60000, 1001).unwrap();
    assert_eq!(ntsc.divided_by(2).unwrap(), Fps::new(30000, 1001).unwrap());
    assert_eq!(ntsc.divided_by(6).unwrap(), Fps::new(10000, 1001).unwrap());

    // Integer divisions
    let fps60 = Fps::new(60, 1).unwrap();
    assert_eq!(fps60.divided_by(2).unwrap(), Fps::new(30, 1).unwrap());
    assert_eq!(fps60.divided_by(4).unwrap(), Fps::new(15, 1).unwrap());
}

#[test]
// Verifies zero factors are rejected before division.
fn rejects_zero_division_factor() {
    let source = Fps::new(60000, 1001).unwrap();

    assert_eq!(
        source.divided_by(0).unwrap_err().to_string(),
        "Encoding error: FPS division factor must be greater than zero"
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
    assert_eq!(Fps::from_f64_fallback(24.0).unwrap().ffmpeg_arg(), "24/1");
    assert_eq!(Fps::from_f64_fallback(48.0).unwrap().ffmpeg_arg(), "48/1");
    assert_eq!(Fps::from_f64_fallback(50.0).unwrap().ffmpeg_arg(), "50/1");
    assert_eq!(Fps::from_f64_fallback(120.0).unwrap().ffmpeg_arg(), "120/1");
    assert_eq!(
        Fps::from_f64_fallback(119.880).unwrap().ffmpeg_arg(),
        "120/1"
    );
    assert_eq!(Fps::from_f64_fallback(47.5).unwrap().ffmpeg_arg(), "48/1");
}

// --- Snapshot / golden tests (Step 11d) ---

#[test]
fn integer_fps_frame_count_for_duration() {
    let fps = Fps::new(30, 1).unwrap();
    assert!((fps.as_f64() - 30.0).abs() < 1e-12);
    assert_eq!((10.0 * fps.as_f64()).ceil() as u64, 300);
}

#[test]
fn zero_denominator_rejected() {
    assert!(Fps::new(30, 0).is_err());
}

#[test]
fn fractional_duration_frame_count() {
    let fps = Fps::new(30, 1).unwrap();
    assert_eq!((7.3 * fps.as_f64()).ceil() as u64, 219);
    assert_eq!((0.033 * fps.as_f64()).ceil() as u64, 1);
}
