/**
 * Builds the per-clip rows of the videos panel.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { useVideoWarnings } from '@/hooks/useVideoTimeline'
import useStore from '@/store/useStore'
import { buildVideoDrawerViewModel, formatOffsetInput } from '../utils/videoDrawerUtils'

/**
 * Returns one row per clip in timeline order: details, its own timeline offset,
 * warnings, and removal. Editing a row's offset moves only that clip.
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
  const setVideoOffset = useStore((state) => state.setVideoOffset)

  return useMemo(
    () =>
      videoSummaries.map((summary) => ({
        id: summary.id,
        filename: summary.filename,
        metadataRows: buildVideoDrawerViewModel(summary, timezone, videoSyncTimezoneMode, t).metadataRows,
        offsetInput: formatOffsetInput(summary.offsetSeconds),
        warnings: (warnings.get(summary.id) ?? []).map((warning) => warning.message),
        onOffsetCommit: (text) => setVideoOffset(summary.id, Math.round(timeToSeconds(text) * 10) / 10),
        onRemove: () => onRemoveVideo(summary.id),
      })),
    [onRemoveVideo, setVideoOffset, t, timezone, videoSummaries, videoSyncTimezoneMode, warnings],
  )
}
