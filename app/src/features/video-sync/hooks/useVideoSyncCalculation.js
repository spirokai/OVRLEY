import { useCallback, useEffect, useEffectEvent, useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES, VIDEO_SYNC_MATCH_SCOPES } from '../data/videoSyncConstants'
import { createActivitySyncInput } from '../utils/activitySyncInput'
import { detectActivityEventsFromInput } from '../utils/detectActivityEvents'
import { getVideoSyncEligibility } from '../utils/landmarkTiming'
import { matchVideoSyncCandidates } from '../utils/intervalConsensus'

function deferCalculation() {
  return new Promise((resolve) => {
    if (typeof window === 'undefined') queueMicrotask(resolve)
    else window.setTimeout(resolve, 0)
  })
}

/**
 * Orchestrates manual video-sync detection and matching outside the store.
 *
 * Detection-only calculations refresh derived activity events after activity
 * or sensitivity changes. Candidate calculations are started explicitly, or
 * rerun automatically after a sensitivity commit when a search already ran.
 *
 * @returns {{calculate: (scope: string) => Promise<boolean>, eligibility: object, isCalculating: boolean}} Calculation view model.
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
  const detectorInput = useMemo(() => (parsedActivity === null ? null : createActivitySyncInput(parsedActivity)), [parsedActivity])
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
      canCalculateAll: hasVideo && domainEligibility.canMatchAll,
      canCalculateLocation: hasVideo && domainEligibility.canMatchLocation,
      allExplanation,
    }
  }, [domainEligibility, hasVideo, parsedActivity, t])

  const refreshDetection = useCallback(async () => {
    if (parsedActivity === null) return false
    const request = {}
    latestDetectionRequest.current = request
    await deferCalculation()
    if (latestDetectionRequest.current !== request) return false

    const current = useStore.getState()
    if (
      current.parsedActivity !== parsedActivity ||
      current.manualVideoSync.speedThresholdKmh !== settings.speedThresholdKmh ||
      current.manualVideoSync.turnThresholdDegrees !== settings.turnThresholdDegrees ||
      (current.manualVideoSyncDetection?.location?.time ?? null) !== (manualVideoSyncDetection?.location?.time ?? null)
    ) {
      return false
    }

    try {
      const detection = detectActivityEventsFromInput(detectorInput, settings, manualVideoSyncDetection?.location ?? null)
      return completeVideoSyncDetection(detection)
    } catch (error) {
      return failVideoSyncDetection(error.message)
    }
  }, [completeVideoSyncDetection, detectorInput, failVideoSyncDetection, manualVideoSyncDetection, parsedActivity, settings])

  const calculate = useCallback(
    async (scope) => {
      if (scope === VIDEO_SYNC_MATCH_SCOPES.ALL && !eligibility.canCalculateAll) return false
      if (scope === VIDEO_SYNC_MATCH_SCOPES.LOCATION && !eligibility.canCalculateLocation) return false

      const revision = beginVideoSyncCalculation(scope)
      if (revision === null) return false
      latestDetectionRequest.current = null
      await deferCalculation()

      try {
        const detection =
          scope === VIDEO_SYNC_MATCH_SCOPES.LOCATION
            ? manualVideoSyncDetection
            : detectActivityEventsFromInput(detectorInput, settings, manualVideoSyncDetection?.location ?? null)
        const { candidates } = matchVideoSyncCandidates({ landmarks, detection, scope })
        return completeVideoSyncCalculation(scope, revision, { detection, candidates })
      } catch (error) {
        return failVideoSyncCalculation(scope, revision, error.message)
      }
    },
    [
      beginVideoSyncCalculation,
      completeVideoSyncCalculation,
      detectorInput,
      eligibility,
      failVideoSyncCalculation,
      landmarks,
      manualVideoSyncDetection,
      settings,
    ],
  )

  useEffect(
    () => () => {
      latestDetectionRequest.current = null
    },
    [],
  )

  const locationSecond = manualVideoSyncDetection?.location?.time ?? null
  const previousInputs = useRef(null)
  const refreshForInputs = useEffectEvent((previous) => {
    if (parsedActivity === null) {
      latestDetectionRequest.current = null
      return
    }

    const settingsChanged =
      previous !== null && (previous.speedThresholdKmh !== speedThresholdKmh || previous.turnThresholdDegrees !== turnThresholdDegrees)
    if (settingsChanged && previous.parsedActivity === parsedActivity && allHasSearched && eligibility.canCalculateAll) {
      void calculate(VIDEO_SYNC_MATCH_SCOPES.ALL)
    } else {
      void refreshDetection()
    }
  })
  useEffect(() => {
    const previous = previousInputs.current
    previousInputs.current = { parsedActivity, speedThresholdKmh, turnThresholdDegrees }
    refreshForInputs(previous)
  }, [parsedActivity, locationSecond, speedThresholdKmh, turnThresholdDegrees])

  return {
    calculate,
    eligibility,
    isCalculating,
  }
}
