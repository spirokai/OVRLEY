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
 * @param {{onRemoveVideo: (videoId: string) => void}} options Removes one clip from the project.
 * @returns {Array<object>} Presentational cards.
 */
export function useVideoCards({ onRemoveVideo }) {
  const { clips } = useVideoTimeline()
  const { selectedLaneId, selectClip } = useStore(
    useShallow((state) => ({ selectedLaneId: state.selectedClip?.laneId ?? null, selectClip: state.selectClip })),
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
        onSelect: () => selectClip({ laneId: videoLaneId(clip.id), segmentKey: null }),
        onRemove: () => onRemoveVideo(clip.id),
      })),
    [clips, onRemoveVideo, selectClip, selectedLaneId],
  )
}
