import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { videoIdFromLaneId, videoLaneId } from '@/features/player/utils/timelineGeometry'
import { resolveEditTarget } from '@/features/player/utils/videoEditTarget'
import { useVideoTimeline } from '@/hooks/useVideoTimeline'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_MAX_LANDMARKS } from '../data/videoSyncConstants'
import { buildVideoSyncDrawerPresentation } from '../utils/videoSyncPresentation'
import useVideoSyncCalculation from './useVideoSyncCalculation'

/**
 * Owns the manual video-sync workspace mode, transient sensitivity controls,
 * and typed landmark actions used by the drawer and preview.
 *
 * The workspace acts on the selected video, or without a selection on the video at
 * the playhead. Marking needs the playhead inside that video. Landmarks belong to
 * one video, which then holds the
 * sync: marking in another video is disabled, and its drawer offers to clear the
 * landmarks or to switch to the video holding them.
 *
 * @param {object} options Workspace inputs.
 * @param {{activeTool: string, visible: boolean}} options.toolbarDrawer Shared toolbar drawer state.
 * @param {object} options.videoSync Existing video-sync control state/actions.
 * @returns {object} Workspace state for the drawer, preview, and player.
 */
export default function useVideoSyncWorkspace({ toolbarDrawer, videoSync }) {
  const { t } = useTranslation()
  const calculation = useVideoSyncCalculation()
  const {
    applyVideoSyncCandidate,
    clearVideoSyncLandmarks,
    clearVideoSyncDetectedLocation,
    hasVideos,
    landmarks,
    manualVideoSyncDetection,
    selectClip,
    selectedVideoId,
    syncTargetId,
    manualVideoSyncResults,
    removeVideoSyncLandmark,
    setVideoSyncDetectedLocation,
    setVideoSyncLandmarkType,
    setVideoSyncSpeedThreshold,
    setVideoSyncTurnThreshold,
    setVideoSyncDrawerTab,
    speedThresholdKmh,
    turnThresholdDegrees,
    videoSyncDrawerTab,
  } = useStore(
    useShallow((state) => ({
      applyVideoSyncCandidate: state.applyVideoSyncCandidate,
      clearVideoSyncLandmarks: state.clearVideoSyncLandmarks,
      clearVideoSyncDetectedLocation: state.clearVideoSyncDetectedLocation,
      hasVideos: state.importedVideos.length > 0,
      landmarks: state.manualVideoSync.landmarks,
      manualVideoSyncDetection: state.manualVideoSyncDetection,
      manualVideoSyncResults: state.manualVideoSyncResults,
      removeVideoSyncLandmark: state.removeVideoSyncLandmark,
      selectClip: state.selectClip,
      selectedVideoId: state.selectedClip === null ? null : videoIdFromLaneId(state.selectedClip.laneId),
      syncTargetId: state.manualVideoSync.videoId,
      setVideoSyncDetectedLocation: state.setVideoSyncDetectedLocation,
      setVideoSyncLandmarkType: state.setVideoSyncLandmarkType,
      setVideoSyncSpeedThreshold: state.setVideoSyncSpeedThreshold,
      setVideoSyncTurnThreshold: state.setVideoSyncTurnThreshold,
      setVideoSyncDrawerTab: state.setVideoSyncDrawerTab,
      speedThresholdKmh: state.manualVideoSync.speedThresholdKmh,
      turnThresholdDegrees: state.manualVideoSync.turnThresholdDegrees,
      videoSyncDrawerTab: state.videoSyncDrawerTab,
    })),
  )

  const { clips, program } = useVideoTimeline()
  const workVideoId = useStore(
    useCallback(
      (state) => selectedVideoId ?? resolveEditTarget(clips, program, null, state.selectedSecond)?.clip.id ?? null,
      [clips, program, selectedVideoId],
    ),
  )
  const workVideo = clips.find((clip) => clip.id === workVideoId) ?? null
  const syncTarget = clips.find((clip) => clip.id === syncTargetId) ?? null
  const isHeldByOtherVideo = syncTarget !== null && syncTarget.id !== workVideoId
  const syncTargetFilename = syncTarget === null ? null : filenameFromSelectedPath(syncTarget.path)

  const [speedThresholdDraftKmh, setSpeedThresholdDraftKmh] = useState(() => speedThresholdKmh)
  const [turnThresholdDraftDegrees, setTurnThresholdDraftDegrees] = useState(() => turnThresholdDegrees)

  useEffect(() => {
    setSpeedThresholdDraftKmh(speedThresholdKmh)
  }, [speedThresholdKmh])

  useEffect(() => {
    setTurnThresholdDraftDegrees(turnThresholdDegrees)
  }, [turnThresholdDegrees])

  const hasLocationLandmark = landmarks.some((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
  const hasLandmarkCapacity = landmarks.length < VIDEO_SYNC_MAX_LANDMARKS
  const drawerPresentation = buildVideoSyncDrawerPresentation(landmarks, manualVideoSyncDetection, {
    speedThresholdDraftKmh,
    turnThresholdDraftDegrees,
  })
  const addLandmarkAtPlayhead = useCallback(
    (type) => {
      const state = useStore.getState()
      state.addVideoSyncLandmark(workVideo.id, type, state.selectedSecond - workVideo.offsetSeconds)
    },
    [workVideo],
  )
  const showSyncTarget = useCallback(
    () => selectClip({ laneId: videoLaneId(syncTargetId), segmentKey: null }, { revealPlayhead: true }),
    [selectClip, syncTargetId],
  )

  const markStop = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.STOP), [addLandmarkAtPlayhead])
  const markLeftTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN), [addLandmarkAtPlayhead])
  const markRightTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN), [addLandmarkAtPlayhead])
  const markLocation = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LOCATION), [addLandmarkAtPlayhead])

  return {
    videoSyncMode: toolbarDrawer.activeTool === VIDEO_SYNC_TOOL,
    drawer: {
      activeTab: videoSyncDrawerTab,
      appliedOffset: syncTarget?.offsetSeconds ?? 0,
      calculation,
      detectionCounts: drawerPresentation.detectionCounts,
      hasLocationLandmark,
      landmarkCards: drawerPresentation.landmarkCards,
      results: manualVideoSyncResults,
      onApplyCandidate: applyVideoSyncCandidate,
      onTabChange: setVideoSyncDrawerTab,
      onClearLandmarks: clearVideoSyncLandmarks,
      onShowSyncVideo: showSyncTarget,
      syncHeldBy: isHeldByOtherVideo ? syncTargetFilename : null,
      onChangeLandmarkType: setVideoSyncLandmarkType,
      onDeleteLandmark: removeVideoSyncLandmark,
      onSpeedThresholdChange: setSpeedThresholdDraftKmh,
      onSpeedThresholdCommit: setVideoSyncSpeedThreshold,
      onTurnThresholdChange: setTurnThresholdDraftDegrees,
      onTurnThresholdCommit: setVideoSyncTurnThreshold,
      speedThresholdDraftKmh,
      speedValueLabel: drawerPresentation.speedValueLabel,
      turnThresholdDraftDegrees,
      turnValueLabel: drawerPresentation.turnValueLabel,
      hasVideos,
      videoSync,
    },
    markControls: {
      detection: manualVideoSyncDetection,
      hasLandmarkCapacity,
      hasLocationLandmark,
      hasVideos,
      isHeldByOtherVideo,
      videoDurationSeconds: workVideo?.durationSeconds ?? null,
      heldByOtherVideoReason: isHeldByOtherVideo
        ? t('videoSync.landmarksInOtherVideo', { defaultValue: 'Landmarks are marked in {{name}}', name: syncTargetFilename })
        : null,
      landmarkLimitReason: t('videoSync.landmarkLimit', 'The maximum of five landmarks has been reached'),
      locationLimitReason: t('videoSync.locationLimit', 'Only one location landmark is allowed'),
      onMarkLeftTurn: markLeftTurn,
      onMarkLocation: markLocation,
      onMarkRightTurn: markRightTurn,
      onMarkStop: markStop,
      onSetCourseLocation: setVideoSyncDetectedLocation,
      onDeleteCourseLocation: clearVideoSyncDetectedLocation,
      playheadOutsideVideoReason: t('videoSync.playheadInsideVideo', 'Move the playhead inside the video to mark a landmark'),
      videoRequiredReason: t('videoSync.videoRequired', 'A video is required to mark a landmark'),
      videoOffsetSeconds: workVideo?.offsetSeconds ?? 0,
    },
  }
}
