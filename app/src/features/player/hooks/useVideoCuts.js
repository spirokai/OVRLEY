/**
 * Cut, exclude and trim commands at the playhead.
 */

import { useCallback, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { secondsToFrames } from '@/lib/video-program/frame-grid'
import { findSegmentAt, planVideoTrim, resolveCutFrame } from '@/lib/video-program/video-segments'
import useStore from '@/store/useStore'
import { videoIdFromLaneId } from '../utils/timelineGeometry'
import { resolveEditTarget, resolveSelectedSegment } from '../utils/videoEditTarget'

/** Trim edit of a target's end at its playhead frame, or null. */
function planTrim(target, side) {
  return target === null ? null : planVideoTrim(target.clip, side, secondsToFrames(target.sourceSecond, target.clip.fps))
}

/**
 * Resolves the clip commands apply to (the selected clip, else the previewed one,
 * else the earliest clip under the playhead) and the commands of the toolbar and
 * shortcuts. Exclusion applies to the selected segment when one is selected, else
 * to the segment at the playhead. A trim excludes the clip's part before (start) or after (end) the playhead
 * (`planVideoTrim`). Commands read the playhead when invoked, so the returned model only
 * changes when a command's availability changes, not on every playback frame.
 *
 * @param {object} options Command inputs.
 * @param {Array<object>} options.clips Committed timeline clips.
 * @param {object} options.program Committed program.
 * @param {number} options.playheadSecond Timeline playhead.
 * @param {function(): number} options.readPlayheadSecond Reads the committed playhead at command time.
 * @param {{laneId: string, segmentKey: string|null}|null} options.selectedClip Timeline selection.
 * @returns {{canCut: boolean, cut: function, excluded: boolean|null, toggleExcluded: function, canTrim: {start: boolean, end: boolean}, trim: function}}
 *   Commands; `excluded` is the export state of the segment exclusion applies to, or null without one.
 */
export default function useVideoCuts({ clips, program, playheadSecond, readPlayheadSecond, selectedClip }) {
  const { cutVideo, moveCut, setVideoSegmentExcluded, trimVideo } = useStore(
    useShallow((state) => ({
      cutVideo: state.cutVideo,
      moveCut: state.moveCut,
      setVideoSegmentExcluded: state.setVideoSegmentExcluded,
      trimVideo: state.trimVideo,
    })),
  )
  const selectedVideoId = selectedClip === null ? null : videoIdFromLaneId(selectedClip.laneId)
  const selectedSegment = resolveSelectedSegment(clips, selectedClip)
  const target = resolveEditTarget(clips, program, selectedVideoId, playheadSecond)
  const canCut = target !== null && resolveCutFrame(target.clip, target.sourceSecond) !== null
  const canTrimStart = planTrim(target, 'start') !== null
  const canTrimEnd = planTrim(target, 'end') !== null
  const excluded =
    selectedSegment !== null ? selectedSegment.segment.excluded : target === null ? null : findSegmentAt(target.clip, target.sourceSecond).excluded

  const cut = useCallback(() => {
    const current = resolveEditTarget(clips, program, selectedVideoId, readPlayheadSecond())
    if (current === null) return
    const frame = resolveCutFrame(current.clip, current.sourceSecond)
    if (frame !== null) cutVideo(current.clip.id, frame)
  }, [clips, cutVideo, program, readPlayheadSecond, selectedVideoId])

  const toggleExcluded = useCallback(() => {
    if (selectedSegment !== null) {
      setVideoSegmentExcluded(selectedSegment.clip.id, selectedSegment.segment.key, !selectedSegment.segment.excluded)
      return
    }
    const current = resolveEditTarget(clips, program, selectedVideoId, readPlayheadSecond())
    if (current === null) return
    const segment = findSegmentAt(current.clip, current.sourceSecond)
    setVideoSegmentExcluded(current.clip.id, segment.key, !segment.excluded)
  }, [clips, program, readPlayheadSecond, selectedSegment, selectedVideoId, setVideoSegmentExcluded])

  const trim = useCallback(
    (side) => {
      const current = resolveEditTarget(clips, program, selectedVideoId, readPlayheadSecond())
      const plan = planTrim(current, side)
      if (plan === null) return
      const frame = secondsToFrames(current.sourceSecond, current.clip.fps)
      if (plan.kind === 'trim') trimVideo(current.clip.id, side, frame)
      else moveCut(current.clip.id, plan.cutId, frame)
    },
    [clips, moveCut, program, readPlayheadSecond, selectedVideoId, trimVideo],
  )
  const canTrim = useMemo(() => ({ start: canTrimStart, end: canTrimEnd }), [canTrimStart, canTrimEnd])

  return useMemo(() => ({ canCut, cut, excluded, toggleExcluded, canTrim, trim }), [canCut, canTrim, cut, excluded, toggleExcluded, trim])
}
