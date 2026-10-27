/**
 * Marks deleted video parts at the playhead: a start, then an end on the same clip.
 */

import { useCallback, useMemo } from 'react'
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

function resolveStartClip(clips, program, selectedClipId, second) {
  const previewVideoId = resolvePreviewTarget(program, second)?.videoId ?? null
  const selectedVideoId = selectedClipId === null ? null : videoIdFromLaneId(selectedClipId)
  return resolveMarkedClip(clips, previewVideoId, selectedVideoId, second)
}

function resolveEndSecond(markedClip, deletedPartMark, second) {
  if (markedClip === null) return null
  const endSecond = clipSecondAt(markedClip, second)
  const fromSecond = Math.min(deletedPartMark.sourceSecond, endSecond)
  const toSecond = Math.max(deletedPartMark.sourceSecond, endSecond)
  return isDeletableRange(markedClip, fromSecond, toSecond) ? endSecond : null
}

/**
 * Resolves the clip a mark applies to (the selected clip, else the previewed one,
 * else the earliest clip under the playhead) and the commands of the toolbar and
 * shortcuts. An end beyond the marked clip deletes up to the clip's edge.
 * Commands read the playhead when invoked, so the returned model only changes
 * when a command becomes available or unavailable, not on every playback frame.
 *
 * @param {object} options Marking inputs.
 * @param {Array<object>} options.clips Committed timeline clips.
 * @param {object} options.program Committed program.
 * @param {number} options.playheadSecond Timeline playhead.
 * @param {function(): number} options.readPlayheadSecond Reads the committed playhead at command time.
 * @param {string|null} options.selectedClipId Selected timeline lane ID.
 * @returns {{mark: {videoId: string, label: string}|null, canStart: boolean, canEnd: boolean, start: function, end: function, clear: function}} Marking commands.
 */
export default function useDeletedPartMarking({ clips, program, playheadSecond, readPlayheadSecond, selectedClipId }) {
  const { deletedPartMark, markDeletedPartStart, completeDeletedPart, clearDeletedPartMark } = useStore(
    useShallow((state) => ({
      deletedPartMark: state.deletedPartMark,
      markDeletedPartStart: state.markDeletedPartStart,
      completeDeletedPart: state.completeDeletedPart,
      clearDeletedPartMark: state.clearDeletedPartMark,
    })),
  )
  const markedClip = useMemo(
    () => (deletedPartMark === null ? null : clips.find((clip) => clip.id === deletedPartMark.videoId)),
    [clips, deletedPartMark],
  )
  const canStart = resolveStartClip(clips, program, selectedClipId, playheadSecond) !== null
  const canEnd = resolveEndSecond(markedClip, deletedPartMark, playheadSecond) !== null
  const mark = useMemo(
    () =>
      markedClip === null
        ? null
        : { videoId: markedClip.id, label: `${basename(markedClip.path)} ${formatClockDuration(deletedPartMark.sourceSecond)}` },
    [deletedPartMark, markedClip],
  )

  const start = useCallback(() => {
    const second = readPlayheadSecond()
    const startClip = resolveStartClip(clips, program, selectedClipId, second)
    if (startClip !== null) markDeletedPartStart(startClip.id, clipSecondAt(startClip, second))
  }, [clips, markDeletedPartStart, program, readPlayheadSecond, selectedClipId])

  const end = useCallback(() => {
    const endSecond = resolveEndSecond(markedClip, deletedPartMark, readPlayheadSecond())
    if (endSecond !== null) completeDeletedPart(endSecond)
  }, [completeDeletedPart, deletedPartMark, markedClip, readPlayheadSecond])

  return useMemo(
    () => ({ mark, canStart, canEnd, start, end, clear: clearDeletedPartMark }),
    [canEnd, canStart, clearDeletedPartMark, end, mark, start],
  )
}
