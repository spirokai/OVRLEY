import { QUALITY_DEFAULTS, QUALITY_SLIDER_RANGE } from '../data/qualityDefaults'
import { EXPORT_CODEC_LOOKUP } from '../data/renderConstants'

/** @param {string} codec Canonical export codec. @returns {number} Default CRF value. */
export function getDefaultQuality(codec) {
  const format = EXPORT_CODEC_LOOKUP[codec].format
  return QUALITY_DEFAULTS[format === 'hevc' ? 'hevc' : 'h264']
}

/** @param {number} value CRF value or slider position. @returns {number} Reversed value within the configured slider range. */
export function invertQualityValue(value) {
  return QUALITY_SLIDER_RANGE.min + QUALITY_SLIDER_RANGE.max - value
}

/** @param {object} settings Render settings entering the store. @returns {void} */
export function validateRenderQuality({ qualityType, qualityValue }) {
  if (qualityType === 'quality') {
    if (!Number.isInteger(qualityValue) || qualityValue < 1 || qualityValue > 51) {
      throw new Error('Render qualityValue must be an integer between 1 and 51 for quality mode')
    }
  } else if (qualityType === 'bitrate') {
    if (!Number.isFinite(qualityValue) || qualityValue <= 0) {
      throw new Error('Render qualityValue must be a positive finite Mbps value for bitrate mode')
    }
  } else {
    throw new Error('Render qualityType must be quality or bitrate')
  }
}
