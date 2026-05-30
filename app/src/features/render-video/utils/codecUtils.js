/**
 * Pure helper functions for render video codec/format/acceleration logic.
 * No React imports — operates on constants from ../data/renderConstants.js.
 */

import { ACCELERATION_OPTIONS, EXPORT_CODEC_LOOKUP, LEGACY_MP4_CODECS, OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'

/**
 * Looks up the output format object for a given export codec string.
 * @param {string} codec - FFmpeg codec name (e.g. 'libx264', 'prores_ks').
 * @returns {object|null} The matching OUTPUT_FORMATS entry, or null.
 */
export function getOutputFormatForExportCodec(codec) {
  return OUTPUT_FORMATS_BY_VALUE[EXPORT_CODEC_LOOKUP[codec]?.format] || null
}

/**
 * Resolves the FFmpeg codec name for a format/acceleration pair.
 * @param {string} formatValue - Format key (e.g. 'h264', 'prores').
 * @param {string} accelerationValue - Acceleration key (e.g. 'cpu', 'nvidia').
 * @returns {string|null} FFmpeg codec name, or null if not found.
 */
export function getExportCodecForSelection(formatValue, accelerationValue) {
  return OUTPUT_FORMATS_BY_VALUE[formatValue]?.codecs?.[accelerationValue] || null
}

/**
 * Checks whether a codec belongs to the MP4 group.
 * @param {string} codec - FFmpeg codec name.
 * @returns {boolean} True if the codec is an MP4-family codec.
 */
export function isMp4Codec(codec) {
  return getOutputFormatForExportCodec(codec)?.group === 'mp4' || LEGACY_MP4_CODECS.includes(codec)
}

/**
 * Maps a codec name to the corresponding availableCodecs flag key.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @param {string} codec - FFmpeg codec name.
 * @returns {boolean} Whether the codec is available.
 */
export function codecFlag(availableCodecs, codec) {
  const flagByCodec = {
    prores_ks: 'proresKs',
    prores_ks_vulkan: 'proresKsVulkan',
    prores_videotoolbox: 'proresVideotoolbox',
    qtrle: 'qtrle',
    libx264: 'libx264',
    libx265: 'libx265',
    h264_nvenc: 'h264Nvenc',
    hevc_nvenc: 'hevcNvenc',
    nnvgpu_h264: 'h264Nvenc',
    nnvgpu_hevc: 'hevcNvenc',
    h264_qsv: 'h264Qsv',
    hevc_qsv: 'hevcQsv',
    qsv_full_h264: 'h264Qsv',
    qsv_full_hevc: 'hevcQsv',
    h264_amf: 'h264Amf',
    hevc_amf: 'hevcAmf',
    h264_videotoolbox: 'h264Videotoolbox',
    hevc_videotoolbox: 'hevcVideotoolbox',
  }
  const key = flagByCodec[codec]
  return Boolean(availableCodecs?.[key])
}

/**
 * Checks whether an acceleration option is visible on the current platform.
 * @param {object} option - An entry from ACCELERATION_OPTIONS.
 * @param {string} platformOs - Current platform ('windows', 'macos', 'linux', 'unknown').
 * @returns {boolean} Whether the option should be shown.
 */
export function isAccelerationPotentiallyVisible(option, platformOs) {
  if (!option?.platform || platformOs === 'unknown') return true
  return option.platform.includes(platformOs)
}

/**
 * Checks whether a specific acceleration method is available for a format.
 * @param {object} format - An entry from OUTPUT_FORMATS.
 * @param {string} accelerationValue - Acceleration key.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @returns {boolean} Whether the acceleration is usable.
 */
export function isAccelerationAvailable(format, accelerationValue, availableCodecs) {
  const codec = getExportCodecForSelection(format.value, accelerationValue)
  if (!codec) return false

  if (!availableCodecs) return false

  if (format.group === 'transparent') {
    if (accelerationValue === 'videotoolbox') {
      return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.videotoolbox)
    }

    return codecFlag(availableCodecs, codec)
  }

  if (accelerationValue === 'nvidia') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.nvgpu || availableCodecs.nnvgpu)
  }

  if (accelerationValue === 'nvidia_cuda') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.nnvgpu)
  }

  if (accelerationValue === 'qsv') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.qsv)
  }

  if (accelerationValue === 'qsv_full') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.qsvFull)
  }

  if (accelerationValue === 'amd') {
    return codecFlag(availableCodecs, codec)
  }

  if (accelerationValue === 'videotoolbox') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.videotoolbox)
  }

  return codecFlag(availableCodecs, codec)
}

/**
 * Returns visible acceleration options for a format, with availability flags.
 * @param {object} format - An entry from OUTPUT_FORMATS.
 * @param {string} platformOs - Current platform.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @returns {Array<object>} Filtered and annotated acceleration options.
 */
export function getVisibleAccelerationOptions(format, platformOs, availableCodecs) {
  return ACCELERATION_OPTIONS.filter((option) => {
    const codecSupported = Object.hasOwn(format.codecs, option.value)
    const platformVisible = isAccelerationPotentiallyVisible(option, platformOs)
    return codecSupported && platformVisible
  }).map((option) => {
    const codecSupported = Object.hasOwn(format.codecs, option.value)
    const platformVisible = isAccelerationPotentiallyVisible(option, platformOs)
    return {
      ...option,
      codecSupported,
      available: codecSupported && platformVisible && isAccelerationAvailable(format, option.value, availableCodecs),
      platformVisible,
    }
  })
}

/**
 * Finds the first available acceleration method for a format.
 * @param {object} format - An entry from OUTPUT_FORMATS.
 * @param {string} platformOs - Current platform.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @returns {object|undefined} The first available acceleration option.
 */
export function getFirstAvailableAcceleration(format, platformOs, availableCodecs) {
  return getVisibleAccelerationOptions(format, platformOs, availableCodecs).find((option) => option.available)
}

/**
 * Checks if an output format is available (has at least one usable acceleration).
 * @param {object} format - An entry from OUTPUT_FORMATS.
 * @param {string} platformOs - Current platform.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @returns {boolean} Whether the format can be used.
 */
export function isOutputFormatAvailable(format, platformOs, availableCodecs) {
  if (format.group === 'transparent') {
    return true
  }

  return Boolean(getFirstAvailableAcceleration(format, platformOs, availableCodecs))
}

/**
 * Finds the first available MP4 export codec across all MP4 formats.
 * @param {string} platformOs - Current platform.
 * @param {object} availableCodecs - Codec availability flags from the backend.
 * @returns {string|null} An FFmpeg codec name, or null if none available.
 */
export function getFirstAvailableMp4ExportCodec(platformOs, availableCodecs) {
  for (const format of OUTPUT_FORMATS.filter((option) => option.group === 'mp4')) {
    const acceleration = getFirstAvailableAcceleration(format, platformOs, availableCodecs)
    if (acceleration) {
      return getExportCodecForSelection(format.value, acceleration.value)
    }
  }

  return null
}

/**
 * Extracts the acceleration key from render settings, defaulting to 'cpu'.
 * @param {object} settings - Current render settings.
 * @returns {string} Acceleration key (e.g. 'cpu', 'nvidia', 'qsv').
 */
export function getAccelerationValueForSettings(settings) {
  const format = getOutputFormatForExportCodec(settings?.exportCodec)
  if (!format) return 'cpu'

  if (settings?.exportAcceleration && getExportCodecForSelection(format.value, settings.exportAcceleration) === settings.exportCodec) {
    return settings.exportAcceleration
  }

  return EXPORT_CODEC_LOOKUP[settings.exportCodec]?.acceleration || 'cpu'
}

/**
 * Checks whether the scene resolution differs from the imported video resolution.
 * @param {object} scene - Scene dimensions ({ width, height }).
 * @param {object} videoResolution - Imported video dimensions ({ width, height }).
 * @returns {boolean} True if resolutions do not match.
 */
export function resolutionsMismatch(scene, videoResolution) {
  if (!scene?.width || !scene?.height || !videoResolution) {
    return false
  }

  return Number(scene.width) !== Number(videoResolution.width) || Number(scene.height) !== Number(videoResolution.height)
}

/**
 * Formats numeric seconds into a mm:ss display string.
 *
 * @param {number|null|undefined} seconds - Numeric seconds value.
 * @returns {string} Formatted representation (e.g. "5:30").
 */
export function formatTime(seconds) {
  if (seconds === null || seconds === undefined) {
    return '--:--'
  }

  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

/**
 * Formats render production FPS for display.
 *
 * @param {number|null|undefined} fps - Numeric frames-per-second value.
 * @returns {string} Formatted FPS value.
 */
export function formatFps(fps) {
  if (fps === null || fps === undefined || !Number.isFinite(fps)) {
    return '--'
  }

  return fps.toFixed(1)
}
