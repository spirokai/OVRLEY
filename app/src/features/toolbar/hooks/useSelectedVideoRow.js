/**
 * Builds the details of the selected video clip (selected on the timeline or in the
 * video panel), shown in the video drawer.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { videoIdFromLaneId } from '@/features/player/utils/timelineGeometry'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { formatVideoWarning } from '@/lib/video-program/video-warnings'
import useStore from '@/store/useStore'
import { buildVideoDrawerViewModel } from '../utils/videoDrawerUtils'
import { buildVideoSegmentRows, groupVideoJoins } from '../utils/videoSegmentRows'

/**
 * Returns the selected clip's row: details, its own timeline offset, warnings, its
 * parts with their export state and transitions, its transitions into the next video
 * (shown at its end, where the timeline draws them), removal, and for a missing video
 * the command that locates its file. Editing the offset moves only that clip.
 *
 * @param {object} options Row inputs.
 * @param {string|null} options.timezone Activity timezone used to display creation times.
 * @param {'local'|'utc'|null} options.videoSyncTimezoneMode Session timezone interpretation.
 * @param {(videoId: string) => void} options.onRemoveVideo Removes one clip from the project.
 * @param {(videoId: string) => void} options.onLocateVideo Asks for the file of a missing video.
 * @returns {object|null} Presentational row, or null when no video is selected.
 */
export function useSelectedVideoRow({ timezone, videoSyncTimezoneMode, onRemoveVideo, onLocateVideo }) {
  const { t } = useTranslation()
  const warnings = useVideoWarnings()
  const { clips, program } = useVideoTimeline()
  const joinModels = useJoinTransitionModels({ clips, program })
  const { selectedVideoId, setVideoOffset, resetVideoOffset, setVideoSegmentExcluded, requestCutRemoval, commitPreviewScrub } = useStore(
    useShallow((state) => ({
      selectedVideoId: state.selectedClip === null ? null : videoIdFromLaneId(state.selectedClip.laneId),
      setVideoOffset: state.setVideoOffset,
      resetVideoOffset: state.resetVideoOffset,
      setVideoSegmentExcluded: state.setVideoSegmentExcluded,
      requestCutRemoval: state.requestCutRemoval,
      commitPreviewScrub: state.commitPreviewScrub,
    })),
  )

  return useMemo(() => {
    const clip = clips.find((candidate) => candidate.id === selectedVideoId)
    if (clip === undefined) return null
    const commands = { setVideoSegmentExcluded, requestCutRemoval, seek: commitPreviewScrub }
    const { partJoinKeys, exitJoinsByVideoId } = groupVideoJoins(program)
    const filenamesById = new Map(clips.map((candidate) => [candidate.id, filenameFromSelectedPath(candidate.path)]))
    return {
      id: clip.id,
      filename: filenameFromSelectedPath(clip.path),
      path: clip.path,
      missing: clip.missing,
      metadataRows: buildVideoDrawerViewModel(clip, timezone, videoSyncTimezoneMode, t).metadataRows,
      // The start snaps to the clip's own frames.
      offsetSeconds: clip.offsetSeconds,
      offsetFps: clip.fps,
      offsetIsManual: clip.offsetSource === 'manual',
      warnings: warnings.get(clip.id).map((warning) => formatVideoWarning(warning, t)),
      onOffsetCommit: (seconds) => setVideoOffset(clip.id, seconds),
      onOffsetReset: () => resetVideoOffset(clip.id),
      onRemove: () => onRemoveVideo(clip.id),
      onLocate: clip.missing ? () => onLocateVideo(clip.id) : null,
      segments: buildVideoSegmentRows({ clip, joinModels, partJoinKeys, commands }),
      // A video without a join into another video has no exits.
      exits: (exitJoinsByVideoId.get(clip.id) ?? []).map((join) => ({
        key: join.key,
        label: t('toolbar.transitionTo', { defaultValue: 'Transition to {{name}}', name: filenamesById.get(join.videoId) }),
        transition: joinModels.get(join.key),
      })),
    }
  }, [
    clips,
    commitPreviewScrub,
    joinModels,
    onLocateVideo,
    onRemoveVideo,
    program,
    requestCutRemoval,
    resetVideoOffset,
    selectedVideoId,
    setVideoOffset,
    setVideoSegmentExcluded,
    t,
    timezone,
    videoSyncTimezoneMode,
    warnings,
  ])
}
