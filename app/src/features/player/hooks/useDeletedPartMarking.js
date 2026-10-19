/**
 * Marks deleted video parts at the playhead: a start, then an end on the same clip.
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { isDeletableRange, resolveMarkedClip } from '@/lib/video-program/deleted-parts'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { formatClockDuration } from '@/lib/time-format'
import useStore from '@/store/useStore'
import { videoIdFromLaneId } from '../utils/timelineGeometry'

function clipSecondAt(clip, second) {
  return Math.min(clip.durationSeconds, Math.max(0, second - clip.offsetSeconds))
}

function basename(path) {
  return path.split(/[\\/]/).pop()
}

/**
 * Resolves the clip a mark applies to (the selected clip, else the previewed one,
 * else the earliest clip under the playhead) and the commands of the toolbar and
 * shortcuts. An end beyond the marked clip deletes up to the clip's edge.
 *
 * @param {object} options Marking inputs.
 * @param {Array<object>} options.clips Committed timeline clips.
 * @param {object} options.program Committed program.
 * @param {number} options.playheadSecond Timeline playhead.
 * @param {string|null} options.selectedClipId Selected timeline lane ID.
 * @returns {{mark: {videoId: string, label: string}|null, canStart: boolean, canEnd: boolean, start: function, end: function, clear: function}} Marking commands.
 */
export default function useDeletedPartMarking({ clips, program, playheadSecond, selectedClipId }) {
  const { deletedPartMark, markDeletedPartStart, completeDeletedPart, clearDeletedPartMark } = useStore(
    useShallow((state) => ({
      deletedPartMark: state.deletedPartMark,
      markDeletedPartStart: state.markDeletedPartStart,
      completeDeletedPart: state.completeDeletedPart,
      clearDeletedPartMark: state.clearDeletedPartMark,
    })),
  )

  return useMemo(() => {
    const previewVideoId = resolvePreviewTarget(program, playheadSecond)?.videoId ?? null
    const selectedVideoId = selectedClipId === null ? null : videoIdFromLaneId(selectedClipId)
    const startClip = resolveMarkedClip(clips, previewVideoId, selectedVideoId, playheadSecond)
    const markedClip = deletedPartMark === null ? null : clips.find((clip) => clip.id === deletedPartMark.videoId)
    const endSecond = markedClip === null ? null : clipSecondAt(markedClip, playheadSecond)
    const canEnd =
      markedClip !== null &&
      isDeletableRange(markedClip, Math.min(deletedPartMark.sourceSecond, endSecond), Math.max(deletedPartMark.sourceSecond, endSecond))
    return {
      mark:
        markedClip === null
          ? null
          : {
              videoId: markedClip.id,
              label: `${basename(markedClip.path)} ${formatClockDuration(deletedPartMark.sourceSecond)}`,
            },
      canStart: startClip !== null,
      canEnd,
      start: () => {
        if (startClip !== null) markDeletedPartStart(startClip.id, clipSecondAt(startClip, playheadSecond))
      },
      end: () => {
        if (canEnd) completeDeletedPart(endSecond)
      },
      clear: clearDeletedPartMark,
    }
  }, [clearDeletedPartMark, clips, completeDeletedPart, deletedPartMark, markDeletedPartStart, playheadSecond, program, selectedClipId])
}
