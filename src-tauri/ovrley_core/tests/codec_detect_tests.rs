//! Codec probing unit tests.
//!
//! Verifies `parse_ffmpeg_filter_names` against synthetic `ffmpeg -filters`
//! output containing CUDA, QSV, and hardware upload/download filter
//! entries. The `detect_codecs` function itself requires live ffmpeg and
//! is tested indirectly through composite pipeline integration tests.
//!
//! ## Type
//! Unit test. No subprocesses — exercises string parsing logic only.
//!
//! ## Regressions guarded
//! - Filter name extraction breaking after ffmpeg output format changes
//! - Missing hardware filter detection (overlay_cuda, scale_qsv, etc.)

use ovrley_core::encode::codec_detect::parse_ffmpeg_filter_names;

#[test]
fn parses_ffmpeg_filter_names_from_filter_listing() {
    let filters = parse_ffmpeg_filter_names(
        " TSC overlay_cuda    VV->V      Overlay one video on top of another using CUDA\n\
         ... scale_cuda      V->V       GPU accelerated video resizer\n\
         ... scale_qsv       V->V       Quick Sync Video scaling and format conversion\n\
         ... hwupload        V->V       Upload a normal frame to a hardware frame\n\
         ... overlay_qsv     VV->V      Quick Sync overlay\n\
         ... hwdownload      V->V       Download a hardware frame\n",
    );

    assert!(filters.contains("overlay_cuda"));
    assert!(filters.contains("scale_cuda"));
    assert!(filters.contains("scale_qsv"));
    assert!(filters.contains("hwupload"));
    assert!(filters.contains("overlay_qsv"));
    assert!(filters.contains("hwdownload"));
}
