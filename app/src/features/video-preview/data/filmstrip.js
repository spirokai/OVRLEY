/**
 * Timeline filmstrip sampling and decoding. Tune the frame spacing and cap here.
 */

/** Source seconds between filmstrip frames. */
export const FILMSTRIP_SAMPLE_SPACING_SECONDS = 20

/** Most frames per video: a longer video spaces its frames wider. */
export const FILMSTRIP_MAX_SAMPLES = 200

/** Height of each decoded frame in pixels: twice the timeline clip height, so frames stay sharp on dense displays. */
export const FILMSTRIP_FRAME_HEIGHT_PX = 96

/** Filmstrips decoded at once; each runs one FFmpeg process. */
export const FILMSTRIP_CONCURRENCY = 2
