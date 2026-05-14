/**
 * Constants for video preview hooks — drift correction, scrub throttling,
 * metadata loading warnings, slow-seek detection, preview delivery mode,
 * and clock mode flag.
 */

/** Maximum allowed drift between the desired playhead and video.currentTime before forcing a seek (seconds). */
export const DRIFT_CORRECTION_SECONDS = 0.25

/** Minimum interval between successive scrub seeks (ms). Rapid scrub requests are coalesced within this window. */
export const SCRUB_SEEK_INTERVAL_MS = 50

/** Epsilon threshold for scrub seeks — skips the currentTime assignment if already within this distance (seconds). */
export const SCRUB_SEEK_EPSILON_SECONDS = 0.05

/** Time after which a mild "Loading video metadata..." warning appears (ms). */
export const METADATA_SOFT_WARNING_MS = 10_000

/** Time after which a stronger metadata-loading warning appears (ms). */
export const METADATA_STRONG_WARNING_MS = 35_000

/** Threshold above which a seek is considered "slow" (ms). */
export const SLOW_SEEK_WARNING_MS = 1_200

/** Number of consecutive slow seeks before the slow-seek warning is shown. */
export const SLOW_SEEK_WARNING_COUNT = 2

/**
 * When true, the preview URL is served via the local HTTP preview server.
 * Set VITE_USE_LOCAL_HTTP_VIDEO_PREVIEW=false to use `convertFileSrc` (direct file:// asset) instead.
 */
export const USE_LOCAL_HTTP_VIDEO_PREVIEW = import.meta.env.VITE_USE_LOCAL_HTTP_VIDEO_PREVIEW !== 'false'

/** localStorage key used to override the clock scheduling strategy (set to 'raf' to force requestAnimationFrame). */
export const PREVIEW_CLOCK_MODE_FLAG = 'ovrley:preview-clock-mode'
