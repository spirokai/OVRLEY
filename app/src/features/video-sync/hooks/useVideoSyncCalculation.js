import { useCallback, useEffect, useEffectEvent, useMemo, useRef } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES } from '../data/videoSyncConstants'
import { createActivitySyncInput } from '../utils/activitySyncInput'
import { detectActivityEvents } from '../utils/detectActivityEvents'
import { getVideoSyncEligibility } from '../utils/landmarkTiming'
import { matchVideoSyncCandidates } from '../utils/intervalConsensus'

function scheduleVideoSyncCalculation({
  activity,
  beginCalculation,
  canCalculate,
  canUseMapOnly,
  completeCalculation,
  completeDetection,
  failCalculation,
  failDetection,
  landmarks,
  searchCandidates,
  settings,
}) {
  if (searchCandidates && !canCalculate) return Promise.resolve(false)
  if (!searchCandidates && activity === null && !canUseMapOnly) return Promise.resolve(false)

  const revision = beginCalculation()
  if (revision === null) return Promise.resolve(false)

  return new Promise((resolve) => {
    const run = () => {
      try {
        const detection = detectActivityEvents(activity, settings)
        if (!searchCandidates) {
          resolve(completeDetection(revision, detection))
          return
        }

        resolve(completeCalculation(revision, { detection, ...matchVideoSyncCandidates(landmarks, detection) }))
      } catch (error) {
        resolve(searchCandidates ? failCalculation(revision, error.message) : failDetection(revision, error.message))
      }
    }

    if (typeof window === 'undefined') queueMicrotask(run)
    else window.setTimeout(run, 0)
  })
}

/**
 * Orchestrates manual video-sync detection and matching outside the store.
 *
 * Detection-only calculations refresh derived activity events after activity
 * or sensitivity changes. Candidate calculations are started explicitly, or
 * rerun automatically after a sensitivity commit when a search already ran.
 *
 * @returns {{calculate: () => Promise<boolean>, eligibility: object, isCalculating: boolean}} Calculation view model.
 */
export default function useVideoSyncCalculation() {
  const {
    beginVideoSyncCalculation,
    completeVideoSyncCalculation,
    completeVideoSyncDetection,
    failVideoSyncCalculation,
    failVideoSyncDetection,
    importedVideoDuration,
    landmarks,
    manualVideoSyncCandidateStatus,
    manualVideoSyncHasSearched,
    parsedActivity,
    speedThresholdKmh,
    turnThresholdDegrees,
  } = useStore(
    useShallow((state) => ({
      beginVideoSyncCalculation: state.beginVideoSyncCalculation,
      completeVideoSyncCalculation: state.completeVideoSyncCalculation,
      completeVideoSyncDetection: state.completeVideoSyncDetection,
      failVideoSyncCalculation: state.failVideoSyncCalculation,
      failVideoSyncDetection: state.failVideoSyncDetection,
      importedVideoDuration: state.importedVideoDuration,
      landmarks: state.manualVideoSync.landmarks,
      manualVideoSyncCandidateStatus: state.manualVideoSyncCandidateStatus,
      manualVideoSyncHasSearched: state.manualVideoSyncHasSearched,
      parsedActivity: state.parsedActivity,
      speedThresholdKmh: state.manualVideoSync.speedThresholdKmh,
      turnThresholdDegrees: state.manualVideoSync.turnThresholdDegrees,
    })),
  )

  const settings = useMemo(() => ({ speedThresholdKmh, turnThresholdDegrees }), [speedThresholdKmh, turnThresholdDegrees])
  const detectorInput = useMemo(() => createActivitySyncInput(parsedActivity), [parsedActivity])
  const domainEligibility = useMemo(() => getVideoSyncEligibility(landmarks, detectorInput.availability), [detectorInput.availability, landmarks])
  const hasVideo = importedVideoDuration !== null
  const eligibility = useMemo(() => {
    let explanation = null
    if (!hasVideo) {
      explanation = 'A video is required for Landmark Sync'
    } else if (!domainEligibility.canUseMapOnly && !domainEligibility.canMatchLandmarks) {
      explanation =
        parsedActivity === null
          ? 'Activity telemetry is required for Landmark Sync'
          : 'At least two usable stop or directional turn landmarks are required'
    }

    return {
      ...domainEligibility,
      canCalculate: hasVideo && (domainEligibility.canMatchLandmarks || domainEligibility.canUseMapOnly),
      explanation,
    }
  }, [domainEligibility, hasVideo, parsedActivity])

  const runCalculation = useCallback(
    (searchCandidates) =>
      scheduleVideoSyncCalculation({
        activity: parsedActivity,
        beginCalculation: beginVideoSyncCalculation,
        canCalculate: eligibility.canCalculate,
        canUseMapOnly: eligibility.canUseMapOnly,
        completeCalculation: completeVideoSyncCalculation,
        completeDetection: completeVideoSyncDetection,
        failCalculation: failVideoSyncCalculation,
        failDetection: failVideoSyncDetection,
        landmarks,
        searchCandidates,
        settings,
      }),
    [
      beginVideoSyncCalculation,
      completeVideoSyncCalculation,
      completeVideoSyncDetection,
      eligibility.canCalculate,
      eligibility.canUseMapOnly,
      failVideoSyncCalculation,
      failVideoSyncDetection,
      landmarks,
      parsedActivity,
      settings,
    ],
  )

  const calculate = useCallback(() => runCalculation(true), [runCalculation])

  const scheduleCalculation = useEffectEvent(runCalculation)
  const previousActivity = useRef(undefined)
  useEffect(() => {
    const activityChanged = previousActivity.current !== parsedActivity
    previousActivity.current = parsedActivity
    if (!activityChanged || parsedActivity === null) return
    void scheduleCalculation(false)
  }, [parsedActivity])

  const previousSettings = useRef(null)
  useEffect(() => {
    const nextSettings = { speedThresholdKmh, turnThresholdDegrees }
    const settingsChanged =
      previousSettings.current !== null &&
      (previousSettings.current.speedThresholdKmh !== nextSettings.speedThresholdKmh ||
        previousSettings.current.turnThresholdDegrees !== nextSettings.turnThresholdDegrees)
    previousSettings.current = nextSettings
    if (!settingsChanged) return

    void scheduleCalculation(manualVideoSyncHasSearched && eligibility.canCalculate)
  }, [eligibility.canCalculate, manualVideoSyncHasSearched, speedThresholdKmh, turnThresholdDegrees])

  return {
    calculate,
    eligibility,
    isCalculating: manualVideoSyncCandidateStatus === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING,
  }
}
