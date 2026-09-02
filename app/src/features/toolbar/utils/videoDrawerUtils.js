import { formatClockDuration } from '@/lib/time-format'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'

const TIME_SOURCE_KEYS = {
  gps: 'toolbar.timeSourceGps',
  ffprobe: 'toolbar.timeSourceMetadata',
  file_mtime: 'toolbar.timeSourceFileModified',
  filename: 'toolbar.timeSourceFilename',
}

/**
 * Builds the display model for imported video metadata.
 *
 * @param {object} videoSummary - Canonical imported video summary.
 * @param {string|null} timezone - Activity timezone used for display.
 * @param {string} timezoneMode - Video sync timezone mode.
 * @param {import('i18next').TFunction} t - Translation function.
 * @returns {{formatLabel: string, metadataRows: Array<{label: string, value: string}>}} Video drawer display model.
 */
export function buildVideoDrawerViewModel(videoSummary, timezone, timezoneMode, t) {
  const unknownValue = t('toolbar.unknown', 'Unknown')
  const formatTimeSource = (source) => {
    if (!source) return unknownValue

    const translationKey = TIME_SOURCE_KEYS[source]
    return translationKey ? t(translationKey) : t('toolbar.unknownTimeSource', { defaultValue: 'Unknown ({{value}})', value: source })
  }

  const metadataRows = [
    { label: t('toolbar.duration', 'Duration'), value: videoSummary.duration ? formatClockDuration(videoSummary.duration) : unknownValue },
    {
      label: t('toolbar.frameRate', 'Frame rate'),
      value: videoSummary.fps ? t('toolbar.valFps', { defaultValue: '{{val}} fps', val: Math.round(videoSummary.fps * 100) / 100 }) : unknownValue,
    },
    {
      label: t('toolbar.resolution', 'Resolution'),
      value: videoSummary.resolution
        ? t('toolbar.valResolution', {
            defaultValue: '{{width}}×{{height}}',
            width: videoSummary.resolution.width,
            height: videoSummary.resolution.height,
          })
        : unknownValue,
    },
    {
      label: t('toolbar.createdAt', 'Created at'),
      value: formatVideoCreationTime(videoSummary.creationTime, videoSummary.timeSource, timezone, timezoneMode) || unknownValue,
    },
    {
      label: t('toolbar.timeSource', 'Time source'),
      value: formatTimeSource(videoSummary.timeSource),
    },
    {
      label: t('toolbar.codec', 'Codec'),
      value: videoSummary.codecName || videoSummary.codecLongName || unknownValue,
    },
    {
      label: t('toolbar.bitRate', 'Bit rate'),
      value: videoSummary.bitRate
        ? t('toolbar.valBitRate', { defaultValue: '{{val}} Mbps', val: (Number(videoSummary.bitRate) / 1_000_000).toFixed(0) })
        : unknownValue,
    },
    {
      label: t('toolbar.camera', 'Camera'),
      value: videoSummary.cameraModel || videoSummary.cameraType || unknownValue,
    },
  ]

  return {
    formatLabel: videoSummary.codecName?.toUpperCase() || unknownValue,
    metadataRows,
  }
}
