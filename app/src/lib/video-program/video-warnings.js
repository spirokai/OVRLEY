/**
 * Per-clip warnings shown in the videos panel and on the timeline.
 */

import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { startsOutsideWindow } from './start-window'
import { deriveCompatibility } from './video-collection'
import { isVideoExcluded } from './video-segments'

const WARNING_TEXT = {
  missing: ['video-preview.warningMissing', 'Video file not found. Locate it or remove the video.'],
  incompatible: ['video-preview.warningIncompatible', 'Different resolution or frame rate than the other videos'],
  overlap: ['video-preview.warningOverlap', 'Overlaps {{name}}'],
  far: ['video-preview.warningFarFromActivity', 'Starts more than an hour before or after the activity'],
}

/**
 * Collects the warnings of every clip: a file that cannot be found, a format that
 * keeps it out of a combined export, overlaps with other clips, and its synchronization warning (outside the
 * activity, unknown recording time). A clip without a synchronization warning was
 * placed by hand, shifted, or placed next to another clip; it is far when it starts
 * outside the start window around the activity (`VIDEO_START_WINDOW_SECONDS`).
 * A clip excluded from export does not take part in the format comparison.
 * @param {Array<object>} clips Timeline clips (videos joined with their edits).
 * @param {{overlaps: Array<{earlierVideoId: string, laterVideoId: string}>}} program Timeline program of the same clips.
 * @param {number|null} activityDurationSeconds External activity duration, or null without an activity.
 * @returns {Map<string, Array<{kind: 'missing'|'incompatible'|'overlap'|'sync'|'far', params: object}>>} Warnings keyed by
 *   video ID (see `formatVideoWarning`); every clip has an entry.
 */
export function deriveVideoWarnings(clips, program, activityDurationSeconds) {
  const { incompatibleIds } = deriveCompatibility(clips.filter((clip) => !isVideoExcluded(clip)))
  const names = new Map(clips.map((clip) => [clip.id, filenameFromSelectedPath(clip.path)]))
  const warnings = new Map(clips.map((clip) => [clip.id, []]))
  for (const clip of clips) {
    if (clip.missing) warnings.get(clip.id).push({ kind: 'missing', params: {} })
    if (incompatibleIds.has(clip.id)) warnings.get(clip.id).push({ kind: 'incompatible', params: {} })
  }
  // One warning per overlapping pair of videos; two videos may share a filename.
  const warnedPairs = new Set()
  for (const { earlierVideoId, laterVideoId } of program.overlaps) {
    for (const [videoId, otherId] of [
      [earlierVideoId, laterVideoId],
      [laterVideoId, earlierVideoId],
    ]) {
      const pair = JSON.stringify([videoId, otherId])
      if (warnedPairs.has(pair)) continue
      warnedPairs.add(pair)
      warnings.get(videoId).push({ kind: 'overlap', params: { name: names.get(otherId) } })
    }
  }
  for (const clip of clips) {
    if (clip.syncWarning !== null) {
      warnings.get(clip.id).push({ kind: 'sync', params: { message: clip.syncWarning } })
    } else if (activityDurationSeconds !== null && startsOutsideWindow(clip.offsetSeconds, { from: 0, to: activityDurationSeconds })) {
      warnings.get(clip.id).push({ kind: 'far', params: {} })
    }
  }
  return warnings
}

/**
 * Words a clip warning for display.
 * @param {{kind: string, params: object}} warning Warning from `deriveVideoWarnings`.
 * @param {function} t Translation function.
 * @returns {string} Message.
 */
export function formatVideoWarning({ kind, params }, t) {
  if (kind === 'sync') return params.message
  const [key, defaultValue] = WARNING_TEXT[kind]
  return t(key, { defaultValue, ...params })
}
