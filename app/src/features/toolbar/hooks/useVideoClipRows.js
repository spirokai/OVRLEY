/**
 * Builds the per-clip rows of the videos panel.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { formatVideoWarning } from '@/lib/video-program/video-warnings'
import useStore from '@/store/useStore'
import { buildVideoDrawerViewModel } from '../utils/videoDrawerUtils'
import { buildVideoSegmentRows, groupVideoJoins } from '../utils/videoSegmentRows'

/**
 * Returns one row per clip in timeline order: details, its own timeline offset,
 * warnings, its parts with their export state and transitions, its transitions into the next
 * video (shown at its end, where the timeline draws them), and removal.
 * Editing a row's offset moves only that clip.
 *
 * @param {object} options Row inputs.
 * @param {string|null} options.timezone Activity timezone used to display creation times.
 * @param {'local'|'utc'|null} options.videoSyncTimezoneMode Session timezone interpretation.
 * @param {(videoId: string) => void} options.onRemoveVideo Removes one clip from the project.
 * @returns {Array<object>} Presentational rows.
 */
export function useVideoClipRows({ timezone, videoSyncTimezoneMode, onRemoveVideo }) {
  const { t } = useTranslation()
  const warnings = useVideoWarnings()
  const { clips, program } = useVideoTimeline()
  const joinModels = useJoinTransitionModels({ clips, program })
  const { setVideoOffset, resetVideoOffset, setVideoSegmentExcluded, requestCutRemoval, commitPreviewScrub } = useStore(
    useShallow((state) => ({
      setVideoOffset: state.setVideoOffset,
      resetVideoOffset: state.resetVideoOffset,
      setVideoSegmentExcluded: state.setVideoSegmentExcluded,
      requestCutRemoval: state.requestCutRemoval,
      commitPreviewScrub: state.commitPreviewScrub,
    })),
  )

  return useMemo(() => {
    const commands = { setVideoSegmentExcluded, requestCutRemoval, seek: commitPreviewScrub }
    const { partJoinKeys, exitJoinsByVideoId } = groupVideoJoins(program)
    const filenamesById = new Map(clips.map((clip) => [clip.id, filenameFromSelectedPath(clip.path)]))
    return sortVideosByTimeline(clips).map((clip) => ({
      id: clip.id,
      filename: filenameFromSelectedPath(clip.path),
      metadataRows: buildVideoDrawerViewModel(clip, timezone, videoSyncTimezoneMode, t).metadataRows,
      // The start snaps to the clip's own frames.
      offsetSeconds: clip.offsetSeconds,
      offsetFps: clip.fps,
      offsetIsManual: clip.offsetSource === 'manual',
      warnings: warnings.get(clip.id).map((warning) => formatVideoWarning(warning, t)),
      onOffsetCommit: (seconds) => setVideoOffset(clip.id, seconds),
      onOffsetReset: () => resetVideoOffset(clip.id),
      onRemove: () => onRemoveVideo(clip.id),
      segments: buildVideoSegmentRows({ clip, joinModels, partJoinKeys, commands }),
      // A video without a join into another video has no exits.
      exits: (exitJoinsByVideoId.get(clip.id) ?? []).map((join) => ({
        key: join.key,
        label: t('toolbar.transitionTo', { defaultValue: 'Transition to {{name}}', name: filenamesById.get(join.videoId) }),
        transition: joinModels.get(join.key),
      })),
    }))
  }, [
    clips,
    commitPreviewScrub,
    joinModels,
    onRemoveVideo,
    program,
    requestCutRemoval,
    resetVideoOffset,
    setVideoOffset,
    setVideoSegmentExcluded,
    t,
    timezone,
    videoSyncTimezoneMode,
    warnings,
  ])
}
