import { formatClockDuration } from '@/lib/time-format'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'

const TELEMETRY_STATUS_LABELS = {
  pending: ['toolbar.telemetryPending', 'Reading…'],
  ready: ['toolbar.telemetryReady', 'Available'],
  absent: ['toolbar.telemetryAbsent', 'None'],
  failed: ['toolbar.telemetryFailed', 'Could not be read'],
}

const TIME_SOURCE_KEYS = {
  gps: 'toolbar.timeSourceGps',
  ffprobe: 'toolbar.timeSourceMetadata',
  filename: 'toolbar.timeSourceFilename',
}

/**
 * Builds the display model for imported video metadata.
 *
 * @param {object} clip - Timeline clip (video joined with its edits).
 * @param {string|null} timezone - Activity timezone used for display.
 * @param {string} timezoneMode - Video sync timezone mode.
 * @param {import('i18next').TFunction} t - Translation function.
 * @returns {{formatLabel: string, metadataRows: Array<{label: string, value: string}>}} Video drawer display model.
 */
export function buildVideoDrawerViewModel(clip, timezone, timezoneMode, t) {
  const unknownValue = t('toolbar.unknown', 'Unknown')
  const formatTimeSource = (source) => {
    if (!source) return unknownValue

    const translationKey = TIME_SOURCE_KEYS[source]
    return translationKey ? t(translationKey) : t('toolbar.unknownTimeSource', { defaultValue: 'Unknown ({{value}})', value: source })
  }

  const metadataRows = [
    { label: t('toolbar.duration', 'Duration'), value: formatClockDuration(clip.durationSeconds) },
    {
      label: t('toolbar.frameRate', 'Frame rate'),
      value: t('toolbar.valFps', { defaultValue: '{{val}} fps', val: Math.round((clip.fps.num / clip.fps.den) * 100) / 100 }),
    },
    {
      label: t('toolbar.resolution', 'Resolution'),
      value: t('toolbar.valResolution', {
        defaultValue: '{{width}}×{{height}}',
        width: clip.resolution.width,
        height: clip.resolution.height,
      }),
    },
    {
      label: t('toolbar.createdAt', 'Created at'),
      value: formatVideoCreationTime(clip.creationTime, clip.timeSource, timezone, timezoneMode) || unknownValue,
    },
    {
      label: t('toolbar.timeSource', 'Time source'),
      value: formatTimeSource(clip.timeSource),
    },
    {
      label: t('toolbar.codec', 'Codec'),
      value: clip.codecName || clip.codecLongName || unknownValue,
    },
    {
      label: t('toolbar.bitRate', 'Bit rate'),
      value: clip.bitRate
        ? t('toolbar.valBitRate', { defaultValue: '{{val}} Mbps', val: (Number(clip.bitRate) / 1_000_000).toFixed(0) })
        : unknownValue,
    },
    {
      label: t('toolbar.camera', 'Camera'),
      value: clip.cameraModel || clip.cameraType || unknownValue,
    },
    {
      label: t('toolbar.telemetry', 'Telemetry'),
      value: t(...TELEMETRY_STATUS_LABELS[clip.telemetry.status]),
    },
  ]

  return {
    formatLabel: clip.codecName?.toUpperCase() || unknownValue,
    metadataRows,
  }
}
