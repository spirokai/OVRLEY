/**
 * Builds the cards of the video panel.
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { videoLaneId } from '@/features/player/utils/timelineGeometry'
import { useVideoTimeline } from '@/hooks/useVideoTimeline'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { formatClockDuration } from '@/lib/time-format'
import { sortVideosByTimeline } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'

/**
 * Returns one card per clip in timeline order: its thumbnail (null when no frame
 * could be decoded), whether its file is missing, selection, and removal. A card
 * shares the clip selection of the timeline: selecting it selects the whole clip.
 * In the video-sync workspace, which acts on the selected video, selecting a card
 * also brings the playhead into its clip, and the card of the video whose
 * landmarks hold the sync is marked.
 * @param {{onRemoveVideo: (videoId: string) => void, syncMode: boolean}} options Removes one clip from the project;
 *   whether the video-sync workspace is open.
 * @returns {Array<object>} Presentational cards.
 */
export function useVideoCards({ onRemoveVideo, syncMode }) {
  const { clips } = useVideoTimeline()
  const { selectedLaneId, selectClip, syncTargetId } = useStore(
    useShallow((state) => ({
      selectedLaneId: state.selectedClip?.laneId ?? null,
      selectClip: state.selectClip,
      syncTargetId: state.manualVideoSync.videoId,
    })),
  )
  return useMemo(
    () =>
      sortVideosByTimeline(clips).map((clip) => ({
        id: clip.id,
        filename: filenameFromSelectedPath(clip.path),
        durationLabel: formatClockDuration(clip.durationSeconds),
        thumbnailUrl: clip.thumbnailUrl,
        missing: clip.missing,
        isSelected: videoLaneId(clip.id) === selectedLaneId,
        holdsSync: syncMode && clip.id === syncTargetId,
        onSelect: () => selectClip({ laneId: videoLaneId(clip.id), segmentKey: null }, { revealPlayhead: syncMode }),
        onRemove: () => onRemoveVideo(clip.id),
      })),
    [clips, onRemoveVideo, selectClip, selectedLaneId, syncMode, syncTargetId],
  )
}
