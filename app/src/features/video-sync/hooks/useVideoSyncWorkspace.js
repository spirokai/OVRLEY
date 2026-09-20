import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_MATCH_SCOPES, VIDEO_SYNC_MAX_LANDMARKS } from '../data/videoSyncConstants'
import useVideoSyncCalculation from './useVideoSyncCalculation'

/**
 * Resolves the landmark controls for the current activity playhead.
 *
 * @param {object|null} markControls Workspace-owned mark action state.
 * @param {number} timelineSecond Current activity timeline second.
 * @param {boolean} enabled Whether the sync workspace is active.
 * @returns {object|null} Resolved mark controls, or null when sync mode is inactive.
 */
export function resolveVideoSyncMarkControls(markControls, timelineSecond, enabled) {
  if (!enabled || markControls === null) return null

  const videoSecond = timelineSecond - markControls.videoSyncOffsetSeconds
  const hasVideo = markControls.importedVideoDuration !== null
  const isInsideVideo = hasVideo && videoSecond >= 0 && videoSecond < markControls.importedVideoDuration
  const canMark = isInsideVideo && markControls.hasLandmarkCapacity
  const markDisabledReason = !hasVideo
    ? markControls.videoRequiredReason
    : !isInsideVideo
      ? markControls.playheadOutsideVideoReason
      : !markControls.hasLandmarkCapacity
        ? markControls.landmarkLimitReason
        : null

  return {
    canMark,
    canMarkLocation: canMark && !markControls.hasLocationLandmark,
    locationDisabledReason: markControls.hasLocationLandmark ? markControls.locationLimitReason : markDisabledReason,
    markDisabledReason,
    onMarkLeftTurn: markControls.onMarkLeftTurn,
    onMarkLocation: markControls.onMarkLocation,
    onMarkRightTurn: markControls.onMarkRightTurn,
    onMarkStop: markControls.onMarkStop,
    detection: markControls.detection,
    onDeleteCourseLocation: markControls.onDeleteCourseLocation,
    onSetCourseLocation: markControls.onSetCourseLocation,
  }
}

/**
 * Owns the manual video-sync workspace mode, transient sensitivity controls,
 * and typed landmark actions used by the drawer and preview.
 *
 * @param {object} options Workspace inputs.
 * @param {{activeTool: string, visible: boolean}} options.toolbarDrawer Shared toolbar drawer state.
 * @param {object} options.videoSummary Imported video summary.
 * @param {object} options.videoSync Existing video-sync control state/actions.
 * @returns {object} Workspace state for the drawer, preview, and player.
 */
export default function useVideoSyncWorkspace({ toolbarDrawer, videoSummary, videoSync }) {
  const { t } = useTranslation()
  const calculation = useVideoSyncCalculation()
  const {
    applyVideoSyncCandidate,
    clearVideoSyncLandmarks,
    clearVideoSyncDetectedLocation,
    importedVideoDuration,
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
    videoSyncOffsetSeconds,
    videoSyncDrawerTab,
  } = useStore(
    useShallow((state) => ({
      applyVideoSyncCandidate: state.applyVideoSyncCandidate,
      clearVideoSyncLandmarks: state.clearVideoSyncLandmarks,
      clearVideoSyncDetectedLocation: state.clearVideoSyncDetectedLocation,
      importedVideoDuration: state.importedVideoDuration,
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
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
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
  const addLandmarkAtPlayhead = useCallback((type) => {
    const state = useStore.getState()
    state.addVideoSyncLandmark(type, state.selectedSecond - state.videoSyncOffsetSeconds)
  }, [])

  const markStop = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.STOP), [addLandmarkAtPlayhead])
  const markLeftTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN), [addLandmarkAtPlayhead])
  const markRightTurn = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN), [addLandmarkAtPlayhead])
  const markLocation = useCallback(() => addLandmarkAtPlayhead(VIDEO_SYNC_LANDMARK_TYPES.LOCATION), [addLandmarkAtPlayhead])

  const deleteLandmark = useCallback((id) => removeVideoSyncLandmark(id), [removeVideoSyncLandmark])
  const deleteDetectedLocation = useCallback(() => clearVideoSyncDetectedLocation(), [clearVideoSyncDetectedLocation])
  const changeLandmarkType = useCallback((id, type) => setVideoSyncLandmarkType(id, type), [setVideoSyncLandmarkType])
  const applyAllCandidate = useCallback((candidate) => applyVideoSyncCandidate(VIDEO_SYNC_MATCH_SCOPES.ALL, candidate), [applyVideoSyncCandidate])
  const applyLocationCandidate = useCallback(
    (candidate) => applyVideoSyncCandidate(VIDEO_SYNC_MATCH_SCOPES.LOCATION, candidate),
    [applyVideoSyncCandidate],
  )

  return {
    videoSyncMode: toolbarDrawer.activeTool === VIDEO_SYNC_TOOL,
    drawer: {
      activeTab: videoSyncDrawerTab,
      appliedOffset: videoSyncOffsetSeconds,
      allResult: manualVideoSyncResults[VIDEO_SYNC_MATCH_SCOPES.ALL],
      calculation,
      landmarks,
      locationResult: manualVideoSyncResults[VIDEO_SYNC_MATCH_SCOPES.LOCATION],
      detection: manualVideoSyncDetection,
      onApplyAllCandidate: applyAllCandidate,
      onApplyLocationCandidate: applyLocationCandidate,
      onTabChange: setVideoSyncDrawerTab,
      onCalculate: calculation.calculateAll,
      onCalculateLocation: calculation.calculateLocation,
      onClearLandmarks: clearVideoSyncLandmarks,
      onChangeLandmarkType: changeLandmarkType,
      onDeleteLandmark: deleteLandmark,
      onSpeedThresholdChange: setSpeedThresholdDraftKmh,
      onSpeedThresholdCommit: setVideoSyncSpeedThreshold,
      onTurnThresholdChange: setTurnThresholdDraftDegrees,
      onTurnThresholdCommit: setVideoSyncTurnThreshold,
      speedThresholdDraftKmh,
      turnThresholdDraftDegrees,
      videoSummary,
      videoSync,
    },
    markControls: {
      detection: manualVideoSyncDetection,
      hasLandmarkCapacity,
      hasLocationLandmark,
      importedVideoDuration,
      landmarkLimitReason: t('videoSync.landmarkLimit', 'The maximum of five landmarks has been reached'),
      locationLimitReason: t('videoSync.locationLimit', 'Only one location landmark is allowed'),
      onMarkLeftTurn: markLeftTurn,
      onMarkLocation: markLocation,
      onMarkRightTurn: markRightTurn,
      onMarkStop: markStop,
      onSetCourseLocation: setVideoSyncDetectedLocation,
      onDeleteCourseLocation: deleteDetectedLocation,
      playheadOutsideVideoReason: t('videoSync.playheadInsideVideo', 'Move the playhead inside the video to mark a landmark'),
      videoRequiredReason: t('videoSync.videoRequired', 'A video is required to mark a landmark'),
      videoSyncOffsetSeconds,
    },
  }
}
