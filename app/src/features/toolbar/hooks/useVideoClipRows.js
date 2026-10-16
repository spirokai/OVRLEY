/**
 * Builds the per-clip rows of the videos panel.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import useStore from '@/store/useStore'
import { buildDeletedPartRows } from '../utils/deletedPartRows'
import { buildVideoDrawerViewModel, formatOffsetInput } from '../utils/videoDrawerUtils'

/**
 * Returns one row per clip in timeline order: details, its own timeline offset,
 * warnings, deleted parts with their join transitions, and removal. Editing a
 * row's offset moves only that clip.
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
  const { setVideoOffset, transitionDefaults, setDeletedPartRange, removeDeletedPart, setJoinTransition, commitPreviewScrub } = useStore(
    useShallow((state) => ({
      setVideoOffset: state.setVideoOffset,
      transitionDefaults: state.transitionDefaults,
      setDeletedPartRange: state.setDeletedPartRange,
      removeDeletedPart: state.removeDeletedPart,
      setJoinTransition: state.setJoinTransition,
      commitPreviewScrub: state.commitPreviewScrub,
    })),
  )

  return useMemo(() => {
    const commands = { setDeletedPartRange, removeDeletedPart, setJoinTransition, seek: commitPreviewScrub }
    return videoSummaries.map((summary) => ({
      id: summary.id,
      filename: summary.filename,
      metadataRows: buildVideoDrawerViewModel(summary, timezone, videoSyncTimezoneMode, t).metadataRows,
      offsetInput: formatOffsetInput(summary.offsetSeconds),
      warnings: (warnings.get(summary.id) ?? []).map((warning) => warning.message),
      onOffsetCommit: (text) => setVideoOffset(summary.id, Math.round(timeToSeconds(text) * 10) / 10),
      onRemove: () => onRemoveVideo(summary.id),
      deletedParts: buildDeletedPartRows({
        clip: clips.find((clip) => clip.id === summary.id),
        program,
        transitionDefaults,
        commands,
        t,
      }),
    }))
  }, [
    clips,
    commitPreviewScrub,
    onRemoveVideo,
    program,
    removeDeletedPart,
    setDeletedPartRange,
    setJoinTransition,
    setVideoOffset,
    t,
    timezone,
    transitionDefaults,
    videoSummaries,
    videoSyncTimezoneMode,
    warnings,
  ])
}
