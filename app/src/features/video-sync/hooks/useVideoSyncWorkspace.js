import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_MAX_LANDMARKS } from '../data/videoSyncConstants'
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
    importedVideoDuration,
    landmarks,
    manualVideoSyncDetection,
    manualVideoSyncCandidateStatus,
    manualVideoSyncCandidates,
    manualVideoSyncError,
    manualVideoSyncHasSearched,
    removeVideoSyncLandmark,
    setVideoSyncLandmarkType,
    setVideoSyncSpeedThreshold,
    setVideoSyncTurnThreshold,
    speedThresholdKmh,
    turnThresholdDegrees,
    videoSyncOffsetSeconds,
  } = useStore(
    useShallow((state) => ({
      applyVideoSyncCandidate: state.applyVideoSyncCandidate,
      clearVideoSyncLandmarks: state.clearVideoSyncLandmarks,
      importedVideoDuration: state.importedVideoDuration,
      landmarks: state.manualVideoSync.landmarks,
      manualVideoSyncDetection: state.manualVideoSyncDetection,
      manualVideoSyncCandidateStatus: state.manualVideoSyncCandidateStatus,
      manualVideoSyncCandidates: state.manualVideoSyncCandidates,
      manualVideoSyncError: state.manualVideoSyncError,
      manualVideoSyncHasSearched: state.manualVideoSyncHasSearched,
      removeVideoSyncLandmark: state.removeVideoSyncLandmark,
      setVideoSyncLandmarkType: state.setVideoSyncLandmarkType,
      setVideoSyncSpeedThreshold: state.setVideoSyncSpeedThreshold,
      setVideoSyncTurnThreshold: state.setVideoSyncTurnThreshold,
      speedThresholdKmh: state.manualVideoSync.speedThresholdKmh,
      turnThresholdDegrees: state.manualVideoSync.turnThresholdDegrees,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
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
  const changeLandmarkType = useCallback((id, type) => setVideoSyncLandmarkType(id, type), [setVideoSyncLandmarkType])
  const applyCandidate = useCallback((candidate) => applyVideoSyncCandidate(candidate), [applyVideoSyncCandidate])

  return {
    videoSyncMode: toolbarDrawer.visible && toolbarDrawer.activeTool === VIDEO_SYNC_TOOL,
    drawer: {
      appliedOffset: videoSyncOffsetSeconds,
      candidates: manualVideoSyncCandidates,
      candidateStatus: manualVideoSyncCandidateStatus,
      calculation,
      error: manualVideoSyncError,
      hasSearched: manualVideoSyncHasSearched,
      landmarks,
      detection: manualVideoSyncDetection,
      onApplyCandidate: applyCandidate,
      onCalculate: calculation.calculate,
      onClearLandmarks: clearVideoSyncLandmarks,
      onChangeLandmarkType: changeLandmarkType,
      onDeleteLandmark: deleteLandmark,
      onSpeedThresholdChange: setSpeedThresholdDraftKmh,
      onSpeedThresholdCommit: setVideoSyncSpeedThreshold,
      onTurnThresholdChange: setTurnThresholdDraftDegrees,
      onTurnThresholdCommit: setVideoSyncTurnThreshold,
      speedThresholdDraftKmh,
      speedThresholdKmh,
      turnThresholdDraftDegrees,
      turnThresholdDegrees,
      videoSummary,
      videoSync,
    },
    markControls: {
      hasLandmarkCapacity,
      hasLocationLandmark,
      importedVideoDuration,
      landmarkLimitReason: t('videoSync.landmarkLimit', 'The maximum of five landmarks has been reached'),
      locationLimitReason: t('videoSync.locationLimit', 'Only one location landmark is allowed'),
      onMarkLeftTurn: markLeftTurn,
      onMarkLocation: markLocation,
      onMarkRightTurn: markRightTurn,
      onMarkStop: markStop,
      playheadOutsideVideoReason: t('videoSync.playheadInsideVideo', 'Move the playhead inside the video to mark a landmark'),
      videoRequiredReason: t('videoSync.videoRequired', 'A video is required to mark a landmark'),
      videoSyncOffsetSeconds,
    },
  }
}
