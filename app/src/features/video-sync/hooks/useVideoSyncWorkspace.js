import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_MAX_LANDMARKS } from '../data/videoSyncConstants'
import useVideoSyncCalculation from './useVideoSyncCalculation'

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
    addVideoSyncLandmark,
    applyVideoSyncCandidate,
    clearVideoSyncLandmarks,
    importedVideoDuration,
    landmarks,
    manualVideoSyncCandidateStatus,
    manualVideoSyncCandidates,
    manualVideoSyncError,
    manualVideoSyncHasSearched,
    removeVideoSyncLandmark,
    selectedSecond,
    setSelectedSecond,
    setVideoSyncSpeedThreshold,
    setVideoSyncTurnThreshold,
    speedThresholdKmh,
    turnThresholdDegrees,
    videoSyncOffsetSeconds,
  } = useStore(
    useShallow((state) => ({
      addVideoSyncLandmark: state.addVideoSyncLandmark,
      applyVideoSyncCandidate: state.applyVideoSyncCandidate,
      clearVideoSyncLandmarks: state.clearVideoSyncLandmarks,
      importedVideoDuration: state.importedVideoDuration,
      landmarks: state.manualVideoSync.landmarks,
      manualVideoSyncCandidateStatus: state.manualVideoSyncCandidateStatus,
      manualVideoSyncCandidates: state.manualVideoSyncCandidates,
      manualVideoSyncError: state.manualVideoSyncError,
      manualVideoSyncHasSearched: state.manualVideoSyncHasSearched,
      removeVideoSyncLandmark: state.removeVideoSyncLandmark,
      selectedSecond: state.selectedSecond,
      setSelectedSecond: state.setSelectedSecond,
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

  const videoSecond = selectedSecond - videoSyncOffsetSeconds
  const hasVideo = importedVideoDuration !== null
  const isInsideVideo = hasVideo && videoSecond >= 0 && videoSecond < importedVideoDuration
  const hasLocationLandmark = landmarks.some((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
  const hasLandmarkCapacity = landmarks.length < VIDEO_SYNC_MAX_LANDMARKS
  const canMark = isInsideVideo && hasLandmarkCapacity
  const markDisabledReason = !hasVideo
    ? t('videoSync.videoRequired', 'A video is required to mark a landmark')
    : !isInsideVideo
      ? t('videoSync.playheadInsideVideo', 'Move the playhead inside the video to mark a landmark')
      : !hasLandmarkCapacity
        ? t('videoSync.landmarkLimit', 'The maximum of five landmarks has been reached')
        : null
  const locationDisabledReason = hasLocationLandmark ? t('videoSync.locationLimit', 'Only one location landmark is allowed') : markDisabledReason

  const markStop = useCallback(() => addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, videoSecond), [addVideoSyncLandmark, videoSecond])
  const markLeftTurn = useCallback(() => addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN, videoSecond), [addVideoSyncLandmark, videoSecond])
  const markRightTurn = useCallback(
    () => addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, videoSecond),
    [addVideoSyncLandmark, videoSecond],
  )
  const markLocation = useCallback(() => addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.LOCATION, videoSecond), [addVideoSyncLandmark, videoSecond])

  const scrubLandmark = useCallback(
    (landmark) => setSelectedSecond(videoSyncOffsetSeconds + landmark.videoSecond),
    [setSelectedSecond, videoSyncOffsetSeconds],
  )
  const deleteLandmark = useCallback((id) => removeVideoSyncLandmark(id), [removeVideoSyncLandmark])
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
      onApplyCandidate: applyCandidate,
      onCalculate: calculation.calculate,
      onClearLandmarks: clearVideoSyncLandmarks,
      onDeleteLandmark: deleteLandmark,
      onScrubLandmark: scrubLandmark,
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
      canMark,
      canMarkLocation: canMark && !hasLocationLandmark,
      locationDisabledReason,
      markDisabledReason,
      onMarkLeftTurn: markLeftTurn,
      onMarkLocation: markLocation,
      onMarkRightTurn: markRightTurn,
      onMarkStop: markStop,
    },
  }
}
