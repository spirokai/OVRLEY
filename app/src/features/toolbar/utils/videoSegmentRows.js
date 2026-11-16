/**
 * View models of a clip's parts (its segments) in the videos panel.
 */

import { formatClockDuration } from '@/lib/time-format'
import { framesToSeconds } from '@/lib/video-program/frame-grid'
import { listVideoSegments, segmentJoinKey } from '@/lib/video-program/video-segments'

/**
 * Groups a program's joins by where the videos panel shows them: a join between two parts of
 * one video among that video's parts, and a join from one video into another at the end of the
 * video it leaves, where the timeline draws it.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @returns {{partJoinKeys: Set<string>, exitJoinsByVideoId: Map<string, Array<object>>}} Keys of joins between parts, and joins
 *   into another video keyed by the video they leave.
 */
export function groupVideoJoins(program) {
  const piecesById = new Map(program.pieces.map((piece) => [piece.id, piece]))
  const partJoinKeys = new Set()
  const exitJoinsByVideoId = new Map()
  for (const join of program.joins) {
    const fromVideoId = piecesById.get(join.fromPieceId).videoId
    if (fromVideoId === join.videoId) {
      partJoinKeys.add(join.key)
      continue
    }
    exitJoinsByVideoId.set(fromVideoId, [...(exitJoinsByVideoId.get(fromVideoId) ?? []), join])
  }
  return { partJoinKeys, exitJoinsByVideoId }
}

/**
 * Builds the rows of a clip's parts in source order. A part has a transition control
 * when a join from an earlier part of the clip leads into it. Removing a part of a cut
 * clip merges it into the part before it (the first part into the part after it),
 * which keeps its own export and overlay state, so removing an excluded part restores
 * that stretch as part of its neighbour.
 * @param {object} options Row inputs.
 * @param {object} options.clip Timeline clip (video joined with its edits).
 * @param {Map<string, object>} options.joinModels Transition controls keyed by join key (`useJoinTransitionModels`).
 * @param {Set<string>} options.partJoinKeys Keys of joins between two parts of one video (`groupVideoJoins`).
 * @param {{setVideoSegmentExcluded: function, setVideoSegmentSkipOverlay: function, requestCutRemoval: function, seek: function}} options.commands
 *   Store commands and playhead seek.
 * @returns {Array<object>} Presentational rows.
 */
export function buildVideoSegmentRows({ clip, joinModels, partJoinKeys, commands }) {
  const segments = listVideoSegments(clip)
  return segments.map((segment, index) => {
    const startSecond = framesToSeconds(segment.startFrame, clip.fps)
    const endSecond = framesToSeconds(segment.endFrame, clip.fps)
    const joinKey = segmentJoinKey(clip.id, segment.key)
    // A cut's ID keys the segment after it: a part starts at its own key's cut and the first part ends at the second part's.
    const merge =
      index > 0
        ? { cutId: segment.key, keep: segments[index - 1].key, into: 'previous' }
        : segments.length > 1
          ? { cutId: segments[1].key, keep: segments[1].key, into: 'next' }
          : null
    return {
      key: segment.key,
      startLabel: formatClockDuration(startSecond),
      endLabel: formatClockDuration(endSecond),
      included: !segment.excluded,
      transition: partJoinKeys.has(joinKey) ? joinModels.get(joinKey) : null,
      onIncludedChange: (included) => commands.setVideoSegmentExcluded(clip.id, segment.key, !included),
      showsOverlay: !segment.skipOverlay,
      onShowsOverlayChange: (shows) => commands.setVideoSegmentSkipOverlay(clip.id, segment.key, !shows),
      onSeek: () => commands.seek(clip.offsetSeconds + startSecond),
      mergesInto: merge?.into ?? null,
      onRemove: merge === null ? null : () => commands.requestCutRemoval(clip.id, merge.cutId, { keep: merge.keep }),
    }
  })
}
