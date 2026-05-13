/**
 * Render video constants, codec configurations, and lookup tables.
 * Constants only — pure helper functions are in ../utils/codecUtils.js.
 */

export const OUTPUT_FORMATS = [
  {
    value: 'prores',
    label: 'ProRes',
    group: 'transparent',
    codecs: {
      cpu: 'prores_ks',
      videotoolbox: 'prores_videotoolbox',
      vulkan_prores: 'prores_ks_vulkan',
    },
  },
  {
    value: 'qtrle',
    label: 'QT RLE',
    group: 'transparent',
    codecs: {
      cpu: 'qtrle',
    },
  },
  {
    value: 'h264',
    label: 'H.264',
    group: 'mp4',
    codecs: {
      cpu: 'libx264',
      nvidia: 'h264_nvenc',
      nvidia_cuda: 'h264_nvenc',
      qsv: 'h264_qsv',
      amd: 'h264_amf',
      videotoolbox: 'h264_videotoolbox',
      vaapi: 'h264_vaapi',
    },
  },
  {
    value: 'hevc',
    label: 'H.265 / HEVC',
    group: 'mp4',
    codecs: {
      cpu: 'libx265',
      nvidia: 'hevc_nvenc',
      nvidia_cuda: 'hevc_nvenc',
      qsv: 'hevc_qsv',
      amd: 'hevc_amf',
      videotoolbox: 'hevc_videotoolbox',
      vaapi: 'hevc_vaapi',
    },
  },
]

export const ACCELERATION_OPTIONS = [
  { value: 'cpu', label: 'CPU' },
  { value: 'nvidia', label: 'NVIDIA GPU', platform: ['windows', 'linux'] },
  {
    value: 'nvidia_cuda',
    label: 'NVIDIA GPU | CUDA',
    platform: ['windows', 'linux'],
  },
  { value: 'qsv', label: 'Intel Quick Sync', platform: ['windows', 'linux'] },
  { value: 'amd', label: 'AMD GPU', platform: ['windows', 'linux'] },
  {
    value: 'videotoolbox',
    label: 'Apple VideoToolbox',
    platform: ['macos'],
  },
  { value: 'vaapi', label: 'VAAPI', platform: ['linux'] },
  { value: 'vulkan_prores', label: 'Vulkan' },
]

export const OUTPUT_FORMATS_BY_VALUE = Object.fromEntries(OUTPUT_FORMATS.map((option) => [option.value, option]))

export const EXPORT_CODEC_LOOKUP = OUTPUT_FORMATS.flatMap((format) =>
  Object.entries(format.codecs).map(([acceleration, codec]) => ({
    codec,
    format: format.value,
    acceleration,
  })),
).reduce((lookup, item) => {
  if (!lookup[item.codec]) {
    lookup[item.codec] = item
  }
  return lookup
}, {})

export const LEGACY_MP4_CODECS = ['h264_vaapi', 'hevc_vaapi']
