/**
 * Pure filmstrip planning: how a video is sampled and which decoder decodes it.
 */

import { FILMSTRIP_FRAME_HEIGHT_PX, FILMSTRIP_MAX_SAMPLES, FILMSTRIP_SAMPLE_SPACING_SECONDS } from '../data/filmstrip'

/**
 * Sampling and frame size of a video's filmstrip: a frame every `FILMSTRIP_SAMPLE_SPACING_SECONDS`, spaced wider so no
 * video exceeds `FILMSTRIP_MAX_SAMPLES` frames, each `FILMSTRIP_FRAME_HEIGHT_PX` tall at the video's display aspect ratio.
 * @param {{durationSeconds: number, resolution: {width: number, height: number}}} video Imported video.
 * @returns {{intervalSeconds: number, sampleCount: number, frameWidth: number, frameHeight: number}} Sample `i` is the
 *   keyframe at or before `i × intervalSeconds`.
 */
export function planFilmstrip(video) {
  const intervalSeconds = Math.max(FILMSTRIP_SAMPLE_SPACING_SECONDS, video.durationSeconds / FILMSTRIP_MAX_SAMPLES)
  return {
    intervalSeconds,
    sampleCount: Math.min(FILMSTRIP_MAX_SAMPLES, Math.ceil(video.durationSeconds / intervalSeconds)),
    frameWidth: Math.round((FILMSTRIP_FRAME_HEIGHT_PX * video.resolution.width) / video.resolution.height),
    frameHeight: FILMSTRIP_FRAME_HEIGHT_PX,
  }
}

/**
 * Hardware decoder of filmstrips, from the startup codec detection: the GPU an export can use, through FFmpeg's own
 * decoders. Windows machines with Intel (QSV) or AMD (AMF) encoders decode through D3D11VA.
 * @param {object|null} codecs Detected codecs (`fetchAvailableCodecs`), or null when detection failed.
 * @returns {'cuda'|'vaapi'|'d3d11va'|'videotoolbox'|null} Decoder, or null to decode in software.
 */
export function filmstripHwaccel(codecs) {
  if (codecs === null) return null
  if (codecs.nvdec) return 'cuda'
  if (codecs.vaapi) return 'vaapi'
  if (codecs.qsv || codecs.h264Amf || codecs.hevcAmf) return 'd3d11va'
  if (codecs.videotoolbox) return 'videotoolbox'
  return null
}
