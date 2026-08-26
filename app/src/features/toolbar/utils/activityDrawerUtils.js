import { formatClockDuration, formatZonedDateTime } from '@/lib/time-format'
import { getActivityAttributeLabel } from '@/lib/widget/widget-icons'

const UNKNOWN_VALUE = 'Unknown'
const METRIC_GROUPS_BY_SOURCE = {
  direct: ['extracted'],
  derived: ['derived'],
  mixed: ['derived'],
}

/**
 * Builds the display model for a canonical activity summary.
 *
 * @param {object} activitySummary - Canonical activity summary normalized by the media store.
 * @returns {{formatLabel: string, metadataRows: Array<{label: string, value: string}>, metricGroups: {derived: string[], extracted: string[]}}} Drawer display model.
 */
export function buildActivityDrawerViewModel(activitySummary) {
  let startTime = UNKNOWN_VALUE
  if (activitySummary.timezone) startTime = formatZonedDateTime(activitySummary.syncTime, activitySummary.timezone)

  let distance = `${Math.round(activitySummary.totalDistanceMeters)} m`
  if (activitySummary.totalDistanceMeters >= 1000) {
    distance = `${(activitySummary.totalDistanceMeters / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 })} km`
  }

  const metadataRows = [
    { label: 'Start time', value: startTime },
    { label: 'Timezone', value: activitySummary.timezone ?? UNKNOWN_VALUE },
    { label: 'Distance', value: distance },
    { label: 'Duration', value: formatClockDuration(activitySummary.durationSeconds) },
    { label: 'Activity type', value: activitySummary.sport ?? UNKNOWN_VALUE },
  ]

  if (activitySummary.originalSampleCount !== null) {
    metadataRows.push({ label: 'Extracted', value: `${activitySummary.originalSampleCount.toLocaleString()} points` })
  }

  const metricGroups = { derived: [], extracted: [] }
  for (const metric of activitySummary.availableMetrics) {
    const label = getActivityAttributeLabel(metric.attribute)
    for (const group of METRIC_GROUPS_BY_SOURCE[metric.source]) metricGroups[group].push(label)
  }
  metricGroups.derived.sort((left, right) => left.localeCompare(right))
  metricGroups.extracted.sort((left, right) => left.localeCompare(right))

  let formatLabel = UNKNOWN_VALUE
  if (activitySummary.fileFormat) formatLabel = activitySummary.fileFormat.toUpperCase()

  return {
    formatLabel,
    metadataRows,
    metricGroups,
  }
}
