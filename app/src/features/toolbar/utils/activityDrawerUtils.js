import { formatClockDuration, formatZonedDateTime } from '@/lib/time-format'
import { getActivityAttributeLabel } from '@/lib/widget/widget-icons'

const METRIC_GROUPS_BY_SOURCE = {
  direct: ['extracted'],
  derived: ['derived'],
  mixed: ['derived'],
}

/**
 * Builds the display model for a canonical activity summary.
 *
 * @param {object} activitySummary - Canonical activity summary normalized by the media store.
 * @param {import('i18next').TFunction} t - Translation function.
 * @returns {{formatLabel: string, metadataRows: Array<{label: string, value: string}>, metricGroups: {derived: string[], extracted: string[]}}} Drawer display model.
 */
export function buildActivityDrawerViewModel(activitySummary, t) {
  const unknownValue = t('toolbar.unknown', 'Unknown')
  let startTime = unknownValue
  if (activitySummary.timezone) startTime = formatZonedDateTime(activitySummary.syncTime, activitySummary.timezone)

  let distance = `${Math.round(activitySummary.totalDistanceMeters).toLocaleString()} m`
  if (activitySummary.totalDistanceMeters >= 1000) {
    distance = `${(activitySummary.totalDistanceMeters / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 })} km`
  }

  const metadataRows = [
    { label: t('toolbar.startTime', 'Start time'), value: startTime },
    { label: t('toolbar.timezone', 'Timezone'), value: activitySummary.timezone ?? unknownValue },
    { label: t('toolbar.distance', 'Distance'), value: distance },
    { label: t('toolbar.duration', 'Duration'), value: formatClockDuration(activitySummary.durationSeconds) },
    { label: t('toolbar.activityType', 'Activity type'), value: activitySummary.sport ?? unknownValue },
  ]

  if (activitySummary.originalSampleCount !== null) {
    metadataRows.push({
      label: t('toolbar.extracted', 'Extracted'),
      value: t('toolbar.valPoints', { defaultValue: '{{val}} points', val: activitySummary.originalSampleCount.toLocaleString() }),
    })
  }

  const metricGroups = { derived: [], extracted: [] }
  for (const metric of activitySummary.availableMetrics) {
    const label = getActivityAttributeLabel(metric.attribute)
    for (const group of METRIC_GROUPS_BY_SOURCE[metric.source]) metricGroups[group].push(label)
  }
  metricGroups.derived.sort((left, right) => left.localeCompare(right))
  metricGroups.extracted.sort((left, right) => left.localeCompare(right))

  let formatLabel = unknownValue
  if (activitySummary.fileFormat) formatLabel = activitySummary.fileFormat.toUpperCase()

  return {
    formatLabel,
    metadataRows,
    metricGroups,
  }
}
