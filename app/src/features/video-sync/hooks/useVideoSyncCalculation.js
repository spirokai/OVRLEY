import { useCallback, useEffect, useEffectEvent, useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES, VIDEO_SYNC_MATCH_SCOPES } from '../data/videoSyncConstants'
import { createActivitySyncInput } from '../utils/activitySyncInput'
import { detectActivityEventsFromInput } from '../utils/detectActivityEvents'
import { getVideoSyncEligibility } from '../utils/landmarkTiming'
import { matchVideoSyncCandidates } from '../utils/intervalConsensus'

function scheduleVideoSyncCalculation({
  beginCalculation,
  eligibility,
  completeCalculation,
  completeDetection,
  currentDetection,
  failCalculation,
  failDetection,
  hasActivity,
  input,
  isCurrent,
  landmarks,
  matchScope,
  settings,
}) {
  if (matchScope === VIDEO_SYNC_MATCH_SCOPES.ALL && !eligibility.canCalculateAll) return Promise.resolve(false)
  if (matchScope === VIDEO_SYNC_MATCH_SCOPES.LOCATION && !eligibility.canCalculateLocation) return Promise.resolve(false)
  if (matchScope === null && !hasActivity) return Promise.resolve(false)

  const revision = matchScope === null ? null : beginCalculation(matchScope)
  if (matchScope !== null && revision === null) return Promise.resolve(false)

  return new Promise((resolve) => {
    const run = () => {
      if (isCurrent !== null && !isCurrent()) {
        resolve(false)
        return
      }
      try {
        const detection =
          matchScope === VIDEO_SYNC_MATCH_SCOPES.LOCATION
            ? currentDetection
            : detectActivityEventsFromInput(input, settings, currentDetection?.location ?? null)
        if (matchScope === null) {
          resolve(completeDetection(detection))
          return
        }

        resolve(completeCalculation(matchScope, revision, { detection, ...matchVideoSyncCandidates({ landmarks, detection, scope: matchScope }) }))
      } catch (error) {
        resolve(matchScope === null ? failDetection(error.message) : failCalculation(matchScope, revision, error.message))
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
 * @returns {{calculateAll: () => Promise<boolean>, calculateLocation: () => Promise<boolean>, eligibility: object, isCalculating: boolean}} Calculation view model.
 */
export default function useVideoSyncCalculation() {
  const { t } = useTranslation()
  const {
    beginVideoSyncCalculation,
    allHasSearched,
    completeVideoSyncCalculation,
    completeVideoSyncDetection,
    failVideoSyncCalculation,
    failVideoSyncDetection,
    importedVideoDuration,
    isCalculating,
    landmarks,
    manualVideoSyncDetection,
    parsedActivity,
    speedThresholdKmh,
    turnThresholdDegrees,
  } = useStore(
    useShallow((state) => ({
      beginVideoSyncCalculation: state.beginVideoSyncCalculation,
      allHasSearched: state.manualVideoSyncResults[VIDEO_SYNC_MATCH_SCOPES.ALL].hasSearched,
      completeVideoSyncCalculation: state.completeVideoSyncCalculation,
      completeVideoSyncDetection: state.completeVideoSyncDetection,
      failVideoSyncCalculation: state.failVideoSyncCalculation,
      failVideoSyncDetection: state.failVideoSyncDetection,
      importedVideoDuration: state.importedVideoDuration,
      isCalculating: Object.values(state.manualVideoSyncResults).some((result) => result.status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING),
      landmarks: state.manualVideoSync.landmarks,
      manualVideoSyncDetection: state.manualVideoSyncDetection,
      parsedActivity: state.parsedActivity,
      speedThresholdKmh: state.manualVideoSync.speedThresholdKmh,
      turnThresholdDegrees: state.manualVideoSync.turnThresholdDegrees,
    })),
  )

  const settings = useMemo(() => ({ speedThresholdKmh, turnThresholdDegrees }), [speedThresholdKmh, turnThresholdDegrees])
  const detectorInput = useMemo(() => createActivitySyncInput(parsedActivity), [parsedActivity])
  const domainEligibility = useMemo(() => getVideoSyncEligibility(landmarks, manualVideoSyncDetection), [landmarks, manualVideoSyncDetection])
  const hasVideo = importedVideoDuration !== null
  const latestDetectionRequest = useRef(null)
  const eligibility = useMemo(() => {
    let allExplanation = null
    if (!hasVideo) {
      allExplanation = t('videoSync.videoRequiredForSync', 'A video is required for Landmark Sync')
    } else if (!domainEligibility.canMatchAll) {
      allExplanation =
        parsedActivity === null
          ? t('videoSync.activityRequiredForSync', 'Activity telemetry is required for Landmark Sync')
          : t('videoSync.minimumLandmarksForSync', 'At least two landmarks are required')
    }

    return {
      ...domainEligibility,
      canCalculateAll: hasVideo && domainEligibility.canMatchAll,
      canCalculateLocation: hasVideo && domainEligibility.canMatchLocation,
      allExplanation,
    }
  }, [domainEligibility, hasVideo, parsedActivity, t])

  const runCalculation = useCallback(
    (matchScope) => {
      let detectionRequest = null
      if (matchScope === null) {
        detectionRequest = {}
        latestDetectionRequest.current = detectionRequest
      } else if (matchScope === VIDEO_SYNC_MATCH_SCOPES.ALL) {
        latestDetectionRequest.current = null
      }

      return scheduleVideoSyncCalculation({
        beginCalculation: beginVideoSyncCalculation,
        eligibility,
        completeCalculation: completeVideoSyncCalculation,
        completeDetection: completeVideoSyncDetection,
        currentDetection: manualVideoSyncDetection,
        failCalculation: failVideoSyncCalculation,
        failDetection: failVideoSyncDetection,
        hasActivity: parsedActivity !== null,
        input: detectorInput,
        isCurrent: detectionRequest === null ? null : () => latestDetectionRequest.current === detectionRequest,
        landmarks,
        matchScope,
        settings,
      })
    },
    [
      beginVideoSyncCalculation,
      completeVideoSyncCalculation,
      completeVideoSyncDetection,
      eligibility,
      failVideoSyncCalculation,
      failVideoSyncDetection,
      detectorInput,
      landmarks,
      manualVideoSyncDetection,
      parsedActivity,
      settings,
    ],
  )

  const calculateAll = useCallback(() => runCalculation(VIDEO_SYNC_MATCH_SCOPES.ALL), [runCalculation])
  const calculateLocation = useCallback(() => runCalculation(VIDEO_SYNC_MATCH_SCOPES.LOCATION), [runCalculation])

  const scheduleCalculation = useEffectEvent(runCalculation)
  useEffect(
    () => () => {
      latestDetectionRequest.current = null
    },
    [],
  )

  const previousActivity = useRef(undefined)
  useEffect(() => {
    const activityChanged = previousActivity.current !== parsedActivity
    previousActivity.current = parsedActivity
    if (!activityChanged) return
    if (parsedActivity === null) {
      latestDetectionRequest.current = null
      return
    }
    void scheduleCalculation(null)
  }, [parsedActivity])

  const locationSecond = manualVideoSyncDetection?.location?.time ?? null
  const previousLocationSecond = useRef(locationSecond)
  useEffect(() => {
    if (previousLocationSecond.current === locationSecond) return
    previousLocationSecond.current = locationSecond
    if (parsedActivity !== null) void scheduleCalculation(null)
  }, [locationSecond, parsedActivity])

  const previousSettings = useRef(null)
  useEffect(() => {
    const nextSettings = { speedThresholdKmh, turnThresholdDegrees }
    const settingsChanged =
      previousSettings.current !== null &&
      (previousSettings.current.speedThresholdKmh !== nextSettings.speedThresholdKmh ||
        previousSettings.current.turnThresholdDegrees !== nextSettings.turnThresholdDegrees)
    previousSettings.current = nextSettings
    if (!settingsChanged) return

    if (allHasSearched && eligibility.canCalculateAll) {
      void scheduleCalculation(VIDEO_SYNC_MATCH_SCOPES.ALL)
    } else {
      void scheduleCalculation(null)
    }
  }, [allHasSearched, eligibility.canCalculateAll, speedThresholdKmh, turnThresholdDegrees])

  return {
    calculateAll,
    calculateLocation,
    eligibility,
    isCalculating,
  }
}
