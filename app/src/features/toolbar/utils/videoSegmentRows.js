/**
 * View models of a clip's parts (its segments) in the videos panel.
 */

import { formatClockDuration } from '@/lib/time-format'
import { framesToSeconds } from '@/lib/video-program/frame-grid'
import { listVideoSegments, segmentJoinKey } from '@/lib/video-program/video-segments'

/**
 * Builds the rows of a clip's parts in source order. A part has a transition control
 * when a join leads into it, and every part but the first starts at a removable cut.
 * @param {object} options Row inputs.
 * @param {object} options.clip Timeline clip (video joined with its edits).
 * @param {Map<string, object>} options.joinModels Transition controls keyed by join key (`useJoinTransitionModels`).
 * @param {{setVideoSegmentExcluded: function, requestCutRemoval: function, seek: function}} options.commands Store commands and playhead seek.
 * @returns {Array<object>} Presentational rows.
 */
export function buildVideoSegmentRows({ clip, joinModels, commands }) {
  return listVideoSegments(clip).map((segment, index) => {
    const startSecond = framesToSeconds(segment.startFrame, clip.fps)
    return {
      key: segment.key,
      startLabel: formatClockDuration(startSecond),
      endLabel: formatClockDuration(framesToSeconds(segment.endFrame, clip.fps)),
      included: !segment.excluded,
      transition: joinModels.get(segmentJoinKey(clip.id, segment.key)) ?? null,
      onIncludedChange: (included) => commands.setVideoSegmentExcluded(clip.id, segment.key, !included),
      onSeek: () => commands.seek(clip.offsetSeconds + startSecond),
      onRemoveCut: index === 0 ? null : () => commands.requestCutRemoval(clip.id, segment.key),
    }
  })
}
