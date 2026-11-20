/** Default CRF values for new projects and codec changes. */
export const QUALITY_DEFAULTS = Object.freeze({
  h264: 21,
  hevc: 26,
})

/** CRF values exposed by the quality slider. */
export const QUALITY_SLIDER_RANGE = Object.freeze({ min: 10, max: 35 })

/**
 * VideoToolbox quality-scale mapping, mirroring `ovrley_core::encode::quality`.
 * The slider's best CRF maps to VideoToolbox quality 100; each CRF step lowers
 * it by the codec's slope. Keep in sync with the Rust constants.
 */
export const VIDEOTOOLBOX_QUALITY_SCALE = Object.freeze({
  maxQuality: 100,
  maxQualityCrf: QUALITY_SLIDER_RANGE.min,
  qualityPerCrf: Object.freeze({
    h264_videotoolbox: 28 / 11,
    hevc_videotoolbox: 30 / 16,
  }),
})
