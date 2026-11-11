/**
 * Builds the details of the selected video clip (selected on the timeline or in the
 * video panel), shown in the video drawer.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { videoIdFromLaneId } from '@/features/player/utils/timelineGeometry'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { formatVideoWarning } from '@/lib/video-program/video-warnings'
import useStore from '@/store/useStore'
import { buildVideoDrawerViewModel } from '../utils/videoDrawerUtils'
import { buildVideoSegmentRows, groupVideoJoins } from '../utils/videoSegmentRows'

/**
 * Returns the selected clip's row: details, warnings, its sync (timeline offset, automatic
 * sync, where its recording time comes from, and how its camera clock is read), its parts
 * with their export state and transitions, its transitions into the next video (shown at
 * its end, where the timeline draws them), removal, and for a missing video the command
 * that locates its file. Every command acts on that clip only.
 *
 * @param {object} options Row inputs.
 * @param {(videoId: string) => void} options.onRemoveVideo Removes one clip from the project.
 * @param {(videoId: string) => void} options.onLocateVideo Asks for the file of a missing video.
 * @returns {object|null} Presentational row, or null when no video is selected.
 */
export function useSelectedVideoRow({ onRemoveVideo, onLocateVideo }) {
  const { t } = useTranslation()
  const warnings = useVideoWarnings()
  const { clips, program } = useVideoTimeline()
  const joinModels = useJoinTransitionModels({ clips, program })
  const {
    selectedVideoId,
    timezone,
    setVideoOffset,
    resetVideoOffset,
    setVideoCreationTimeSource,
    setVideoTimezoneMode,
    setVideoSegmentExcluded,
    requestCutRemoval,
    commitPreviewScrub,
  } = useStore(
    useShallow((state) => ({
      selectedVideoId: state.selectedClip === null ? null : videoIdFromLaneId(state.selectedClip.laneId),
      timezone: state.parsedActivity?.metadata?.timezone ?? null,
      setVideoOffset: state.setVideoOffset,
      resetVideoOffset: state.resetVideoOffset,
      setVideoCreationTimeSource: state.setVideoCreationTimeSource,
      setVideoTimezoneMode: state.setVideoTimezoneMode,
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
      metadataRows: buildVideoDrawerViewModel(clip, timezone, clip.timezoneMode, t).metadataRows,
      // The start snaps to the clip's own frames.
      offsetSeconds: clip.offsetSeconds,
      offsetFps: clip.fps,
      warnings: warnings.get(clip.id).map((warning) => formatVideoWarning(warning, t)),
      onOffsetCommit: (seconds) => setVideoOffset(clip.id, seconds),
      // Syncing places the clip by its detected recording time again, also after it was set by hand.
      onSync: () => resetVideoOffset(clip.id),
      creationTimeSource: clip.timeSource === 'filename' ? 'filename' : 'detected',
      filenameCreationTimeAvailable: parseVideoFilenameCreationTime(clip.path) !== null,
      onCreationTimeSourceChange: (source) => setVideoCreationTimeSource(clip.id, source),
      // Apply Timezone needs the activity's timezone; a filename timestamp is always local time.
      timezone,
      appliesTimezone: clip.timezoneMode === 'utc',
      timezoneDisabled: clip.timeSource === 'filename',
      onAppliesTimezoneChange: (applies) => setVideoTimezoneMode(clip.id, applies ? 'utc' : 'local'),
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
    setVideoCreationTimeSource,
    setVideoOffset,
    setVideoSegmentExcluded,
    setVideoTimezoneMode,
    t,
    timezone,
    warnings,
  ])
}
