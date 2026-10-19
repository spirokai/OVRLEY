/**
 * Activity summary derivation shared by external activities and per-video embedded telemetry.
 */

const summaries = new WeakMap()

/**
 * Returns the playable activity duration in seconds.
 * @param {object} activity Finalized parsed activity.
 * @returns {number} Duration in seconds, or 0 when unknown.
 */
export function getActivityDurationSeconds(activity) {
  const duration = Number(activity?.trim_end_seconds ?? activity?.metadata?.duration_seconds ?? 0)
  return Number.isFinite(duration) ? duration : 0
}

/**
 * Builds the summary used by the timeline, widgets, and synchronization.
 * Summaries are cached per activity object, so repeated calls return the same object.
 * @param {object} activity Finalized parsed activity.
 * @returns {object} Activity summary.
 */
export function createActivitySummary(activity) {
  const cached = summaries.get(activity)
  if (cached) return cached

  const summary = {
    availableMetrics: [...activity.valid_attributes, ...activity.extended_attributes].map((attribute) => ({
      attribute,
      source: activity.coverage[attribute].source,
    })),
    durationSeconds: getActivityDurationSeconds(activity),
    endTime: activity.metadata?.end_time ?? null,
    fileFormat: activity.file_format,
    fileName: activity.file_name,
    originalSampleCount: activity.metadata?.original_sample_count ?? null,
    sampleCount: activity.metadata?.sample_count ?? 0,
    sport: activity.metadata?.sport ?? null,
    syncTime: activity.sync_time ?? null,
    timezone: activity.metadata?.timezone ?? null,
    totalDistanceMeters: activity.metadata?.total_distance_m ?? 0,
  }
  summaries.set(activity, summary)
  return summary
}
