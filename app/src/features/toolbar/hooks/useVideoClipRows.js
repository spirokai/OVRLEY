/**
 * Builds the per-clip rows of the videos panel.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import useStore from '@/store/useStore'
import { buildDeletedPartRows } from '../utils/deletedPartRows'
import { buildVideoDrawerViewModel, formatOffsetInput } from '../utils/videoDrawerUtils'

/**
 * Returns one row per clip in timeline order: details, its own timeline offset,
 * warnings, the transition into the clip, deleted parts with their join
 * transitions, and removal. Editing a row's offset moves only that clip.
 *
 * @param {object} options Row inputs.
 * @param {Array<object>} options.videoSummaries Clip summaries in timeline order.
 * @param {string|null} options.timezone Activity timezone used to display creation times.
 * @param {'local'|'utc'|null} options.videoSyncTimezoneMode Session timezone interpretation.
 * @param {(videoId: string) => void} options.onRemoveVideo Removes one clip.
 * @returns {Array<object>} Presentational rows.
 */
export function useVideoClipRows({ videoSummaries, timezone, videoSyncTimezoneMode, onRemoveVideo }) {
  const { t } = useTranslation()
  const warnings = useVideoWarnings()
  const { clips, program } = useVideoTimeline()
  const joinModels = useJoinTransitionModels({ clips, program })
  const { setVideoOffset, setDeletedPartRange, removeDeletedPart, commitPreviewScrub } = useStore(
    useShallow((state) => ({
      setVideoOffset: state.setVideoOffset,
      setDeletedPartRange: state.setDeletedPartRange,
      removeDeletedPart: state.removeDeletedPart,
      commitPreviewScrub: state.commitPreviewScrub,
    })),
  )

  return useMemo(() => {
    const commands = { setDeletedPartRange, removeDeletedPart, seek: commitPreviewScrub }
    return videoSummaries.map((summary) => ({
      id: summary.id,
      filename: summary.filename,
      metadataRows: buildVideoDrawerViewModel(summary, timezone, videoSyncTimezoneMode, t).metadataRows,
      offsetInput: formatOffsetInput(summary.offsetSeconds),
      warnings: (warnings.get(summary.id) ?? []).map((warning) => warning.message),
      onOffsetCommit: (text) => setVideoOffset(summary.id, Math.round(timeToSeconds(text) * 10) / 10),
      onRemove: () => onRemoveVideo(summary.id),
      // Only a clip that follows another piece has a join leading into it.
      transitionIn: joinModels.get(`clip:${summary.id}`) ?? null,
      deletedParts: buildDeletedPartRows({ clip: clips.find((clip) => clip.id === summary.id), joinModels, commands, t }),
    }))
  }, [
    clips,
    commitPreviewScrub,
    joinModels,
    onRemoveVideo,
    removeDeletedPart,
    setDeletedPartRange,
    setVideoOffset,
    t,
    timezone,
    videoSummaries,
    videoSyncTimezoneMode,
    warnings,
  ])
}
