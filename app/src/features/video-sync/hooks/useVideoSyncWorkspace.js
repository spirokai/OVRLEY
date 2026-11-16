import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { findVideoSyncTarget } from '@/store/slices/createManualVideoSyncSlice'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_MAX_LANDMARKS } from '../data/videoSyncConstants'
import { buildVideoSyncDrawerPresentation } from '../utils/videoSyncPresentation'
import useVideoSyncCalculation from './useVideoSyncCalculation'

/**
 * Owns the manual video-sync workspace mode, transient sensitivity controls,
 * and typed landmark actions used by the drawer and preview.
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
    manualVideoSyncResults,
    removeVideoSyncLandmark,
    setVideoSyncDetectedLocation,
    setVideoSyncLandmarkType,
    setVideoSyncSpeedThreshold,
    setVideoSyncTurnThreshold,
    setVideoSyncDrawerTab,
    speedThresholdKmh,
    turnThresholdDegrees,
    targetDurationSeconds,
    targetOffsetSeconds,
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
      setVideoSyncDetectedLocation: state.setVideoSyncDetectedLocation,
      setVideoSyncLandmarkType: state.setVideoSyncLandmarkType,
      setVideoSyncSpeedThreshold: state.setVideoSyncSpeedThreshold,
      setVideoSyncTurnThreshold: state.setVideoSyncTurnThreshold,
      setVideoSyncDrawerTab: state.setVideoSyncDrawerTab,
      speedThresholdKmh: state.manualVideoSync.speedThresholdKmh,
      turnThresholdDegrees: state.manualVideoSync.turnThresholdDegrees,
      targetDurationSeconds: findVideoSyncTarget(state)?.durationSeconds ?? null,
      targetOffsetSeconds: findVideoSyncTarget(state)?.offsetSeconds ?? 0,
      videoSyncDrawerTab: state.videoSyncDrawerTab,
    })),
  )

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
  const addLandmarkAtPlayhead = useCallback((type) => {
    const state = useStore.getState()
    state.addVideoSyncLandmark(type, state.selectedSecond - findVideoSyncTarget(state).offsetSeconds)
  }, [])

  const markStop = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.STOP), [addLandmarkAtPlayhead])
  const markLeftTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN), [addLandmarkAtPlayhead])
  const markRightTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN), [addLandmarkAtPlayhead])
  const markLocation = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LOCATION), [addLandmarkAtPlayhead])

  return {
    videoSyncMode: toolbarDrawer.activeTool === VIDEO_SYNC_TOOL,
    drawer: {
      activeTab: videoSyncDrawerTab,
      appliedOffset: targetOffsetSeconds,
      calculation,
      detectionCounts: drawerPresentation.detectionCounts,
      hasLocationLandmark,
      landmarkCards: drawerPresentation.landmarkCards,
      results: manualVideoSyncResults,
      onApplyCandidate: applyVideoSyncCandidate,
      onTabChange: setVideoSyncDrawerTab,
      onClearLandmarks: clearVideoSyncLandmarks,
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
      videoDurationSeconds: targetDurationSeconds,
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
      videoOffsetSeconds: targetOffsetSeconds,
    },
  }
}
