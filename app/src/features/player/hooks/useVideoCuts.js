/**
 * Cut and exclude commands at the playhead.
 */

import { useCallback, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { findSegmentAt, resolveCutFrame } from '@/lib/video-program/video-segments'
import useStore from '@/store/useStore'
import { videoIdFromLaneId } from '../utils/timelineGeometry'
import { resolveEditTarget, resolveSelectedSegment } from '../utils/videoEditTarget'

/**
 * Resolves the clip commands apply to (the selected clip, else the previewed one,
 * else the earliest clip under the playhead) and the commands of the toolbar and
 * shortcuts. Exclusion applies to the selected segment when one is selected, else
 * to the segment at the playhead. Commands read the playhead when invoked, so the returned model only
 * changes when a command's availability changes, not on every playback frame.
 *
 * @param {object} options Command inputs.
 * @param {Array<object>} options.clips Committed timeline clips.
 * @param {object} options.program Committed program.
 * @param {number} options.playheadSecond Timeline playhead.
 * @param {function(): number} options.readPlayheadSecond Reads the committed playhead at command time.
 * @param {{laneId: string, segmentKey: string|null}|null} options.selectedClip Timeline selection.
 * @returns {{canCut: boolean, cut: function, excluded: boolean|null, toggleExcluded: function}} Commands; `excluded` is
 *   the export state of the segment exclusion applies to, or null without one.
 */
export default function useVideoCuts({ clips, program, playheadSecond, readPlayheadSecond, selectedClip }) {
  const { cutVideo, setVideoSegmentExcluded } = useStore(
    useShallow((state) => ({ cutVideo: state.cutVideo, setVideoSegmentExcluded: state.setVideoSegmentExcluded })),
  )
  const selectedVideoId = selectedClip === null ? null : videoIdFromLaneId(selectedClip.laneId)
  const selectedSegment = resolveSelectedSegment(clips, selectedClip)
  const target = resolveEditTarget(clips, program, selectedVideoId, playheadSecond)
  const canCut = target !== null && resolveCutFrame(target.clip, target.sourceSecond) !== null
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

  return useMemo(() => ({ canCut, cut, excluded, toggleExcluded }), [canCut, cut, excluded, toggleExcluded])
}
