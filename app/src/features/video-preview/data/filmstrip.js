/**
 * Timeline filmstrip sampling and decoding. The grid constants mirror `src-tauri/src/video_filmstrip.rs`, which plans
 * the frames of each strip and refuses windows off this grid.
 */

/** Most frames in one strip: the base strip of a video, or one zoom window. */
export const FILMSTRIP_FRAMES_PER_STRIP = 24

/** Closest spacing of frames, on the finest zoom level (level 0); level `k` spaces them `2^k` times wider. */
export const FILMSTRIP_FINEST_SPACING_SECONDS = 10

/** Height of each decoded frame in pixels: twice the timeline clip height, so frames stay sharp on dense displays. */
export const FILMSTRIP_FRAME_HEIGHT_PX = 96

/** Strips decoded at once; each runs one FFmpeg process. Cache reads do not wait for a slot. */
export const FILMSTRIP_CONCURRENCY = 2

/** Zoom windows kept in memory before those the timeline no longer shows are dropped. */
export const FILMSTRIP_WINDOWS_KEPT = 96
