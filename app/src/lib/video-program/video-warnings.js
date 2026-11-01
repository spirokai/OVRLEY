/**
 * Per-clip warnings shown in the videos panel and on the timeline.
 */

import i18next from 'i18next'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { startsOutsideWindow } from './start-window'
import { deriveCompatibility } from './video-collection'

/**
 * Collects the warnings of every clip: a format that keeps it out of a combined
 * export, overlaps with other clips, and its synchronization warning (outside the
 * activity, unknown recording time). A clip without a synchronization warning was
 * placed by hand, shifted, or placed next to another clip; it is far when it starts
 * outside the start window around the activity (`VIDEO_START_WINDOW_SECONDS`).
 * @param {Array<object>} clips Timeline clips (videos joined with their edits).
 * @param {{overlaps: Array<{earlierVideoId: string, laterVideoId: string}>}} program Timeline program of the same clips.
 * @param {number|null} activityDurationSeconds External activity duration, or null without an activity.
 * @returns {Map<string, Array<{kind: 'incompatible'|'overlap'|'sync'|'far', message: string}>>} Warnings keyed by video ID; every
 *   clip has an entry.
 */
export function deriveVideoWarnings(clips, program, activityDurationSeconds) {
  const { incompatibleIds } = deriveCompatibility(clips)
  const names = new Map(clips.map((clip) => [clip.id, filenameFromSelectedPath(clip.path)]))
  const warnings = new Map(clips.map((clip) => [clip.id, []]))
  for (const clip of clips) {
    if (incompatibleIds.has(clip.id)) {
      warnings.get(clip.id).push({
        kind: 'incompatible',
        message: i18next.t('video-preview.warningIncompatible', 'Different resolution or frame rate than the other videos'),
      })
    }
  }
  for (const { earlierVideoId, laterVideoId } of program.overlaps) {
    for (const [videoId, otherId] of [
      [earlierVideoId, laterVideoId],
      [laterVideoId, earlierVideoId],
    ]) {
      const message = i18next.t('video-preview.warningOverlap', { defaultValue: 'Overlaps {{name}}', name: names.get(otherId) })
      if (!warnings.get(videoId).some((warning) => warning.message === message)) warnings.get(videoId).push({ kind: 'overlap', message })
    }
  }
  for (const clip of clips) {
    if (clip.syncWarning !== null) {
      warnings.get(clip.id).push({ kind: 'sync', message: clip.syncWarning })
    } else if (activityDurationSeconds !== null && startsOutsideWindow(clip.offsetSeconds, { from: 0, to: activityDurationSeconds })) {
      warnings.get(clip.id).push({
        kind: 'far',
        message: i18next.t('video-preview.warningFarFromActivity', 'Starts more than an hour before or after the activity'),
      })
    }
  }
  return warnings
}
