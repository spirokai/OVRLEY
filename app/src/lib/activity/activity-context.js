/**
 * Resolves the activity that drives widgets at a timeline second.
 *
 * An external activity is authoritative for the whole timeline and starts at
 * timeline second 0. Without it, the video shown at the playhead supplies its own
 * embedded telemetry in video-local time; telemetry is never borrowed from
 * another video.
 */

import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { createActivitySummary } from './activity-summary'

/**
 * Returns the clip whose embedded telemetry drives widgets at a timeline second:
 * the clip shown by the preview there.
 * @param {object} program Timeline program.
 * @param {number} second Timeline second.
 * @returns {string|null} Clip ID, or null without clips.
 */
export function resolveActivityClipId(program, second) {
  return resolvePreviewTarget(program, second)?.videoId ?? null
}

/**
 * @param {object} options Activity inputs.
 * @param {object|null} options.parsedActivity External activity.
 * @param {object|null} options.activitySummary External activity summary.
 * @param {object|null} options.clip Timeline clip shown at the second of interest (see resolveActivityClipId).
 * @returns {{activity: object|null, summary: object|null, originSecond: number, videoId: string|null}|null}
 *   Activity context: `activity` is null while the clip's telemetry is pending, absent, or failed. Null without activity and clips.
 */
export function resolveActivityContext({ parsedActivity, activitySummary, clip }) {
  if (parsedActivity !== null) return { activity: parsedActivity, summary: activitySummary, originSecond: 0, videoId: null }
  if (clip === null) return null
  const activity = clip.telemetry.activity
  return {
    activity,
    summary: activity === null ? null : createActivitySummary(activity),
    originSecond: clip.offsetSeconds,
    videoId: clip.id,
  }
}

/**
 * Metrics available for new widgets: the external activity's metrics, or the
 * union of every video's embedded telemetry metrics.
 * @param {object|null} activitySummary External activity summary.
 * @param {Array<object>} videos Imported videos.
 * @returns {Array<{attribute: string, source: string}>} Available metrics.
 */
export function resolveAvailableMetrics(activitySummary, videos) {
  if (activitySummary !== null) return activitySummary.availableMetrics
  const metrics = new Map()
  for (const video of videos) {
    if (video.telemetry.activity === null) continue
    for (const metric of createActivitySummary(video.telemetry.activity).availableMetrics) {
      if (!metrics.has(metric.attribute)) metrics.set(metric.attribute, metric)
    }
  }
  return [...metrics.values()]
}
