import {
  MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES,
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_DETECTED_LOCATION_ID,
  VIDEO_SYNC_MAX_LANDMARKS,
  VIDEO_SYNC_MAX_LOCATION_LANDMARKS,
  VIDEO_SYNC_MATCH_SCOPES,
  VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH,
  VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES,
} from '@/features/video-sync/data/videoSyncConstants'
import {
  cloneManualState,
  createDefaultManualState,
  validateActivitySecond,
  validateLandmarkId,
  validateLandmark,
  validateThreshold,
  validateVideoSecond,
} from '@/features/video-sync/utils/manualVideoSyncContract'
import { createEmptyVideoSyncDetection } from '@/features/video-sync/utils/detectActivityEvents'

function createLandmarkId() {
  const randomUUID = globalThis.crypto?.randomUUID
  if (typeof randomUUID !== 'function') {
    throw new Error('The runtime does not provide a UUID generator for manual video sync landmarks')
  }
  return validateLandmarkId(randomUUID.call(globalThis.crypto))
}

function requireRevision(revision) {
  if (!Number.isInteger(revision) || revision < 0) {
    throw new Error('Manual video sync calculation revision must be a non-negative integer')
  }
}

function requireMatchScope(scope) {
  if (!Object.values(VIDEO_SYNC_MATCH_SCOPES).includes(scope)) {
    throw new Error(`Unsupported video sync match scope: ${String(scope)}`)
  }
}

function createCandidateResult() {
  return {
    candidates: [],
    error: null,
    hasSearched: false,
    revision: 0,
    status: MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE,
  }
}

function createCandidateResults() {
  return {
    [VIDEO_SYNC_MATCH_SCOPES.ALL]: createCandidateResult(),
    [VIDEO_SYNC_MATCH_SCOPES.LOCATION]: createCandidateResult(),
  }
}

function invalidateResult(draft, scope) {
  const result = draft.manualVideoSyncResults[scope]
  result.revision += 1
  result.error = null
  if (result.hasSearched) result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.STALE
  else result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE
}

function invalidateAllResults(draft) {
  invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
  invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.LOCATION)
}

function resetDerivedState(draft) {
  draft.manualVideoSyncDetection = null
  draft.manualVideoSyncResults = createCandidateResults()
}

function requireCalculationResult(result) {
  if (!result || typeof result !== 'object' || Array.isArray(result) || !Object.hasOwn(result, 'detection') || !Array.isArray(result.candidates)) {
    throw new Error('Manual video sync calculation result must include detection and candidates')
  }
}

function requireCalculationError(message) {
  if (typeof message !== 'string' || message.trim() === '') {
    throw new Error('Manual video sync calculation error must be a non-empty string')
  }
}

/**
 * Creates the manual video-sync store slice.
 * @param {Function} set Zustand setter callback.
 * @param {Function} get Zustand getter callback.
 * @returns {object} Manual video-sync state and domain actions.
 */
export function createManualVideoSyncSlice(set, get) {
  return {
    manualVideoSync: createDefaultManualState(),
    manualVideoSyncDetection: null,
    manualVideoSyncResults: createCandidateResults(),

    addVideoSyncLandmark: (type, videoSecond) => {
      const state = get()
      if (state.importedVideoDuration === null) {
        throw new Error('A video must be loaded before adding a manual video sync landmark')
      }
      const id = createLandmarkId()
      if (state.manualVideoSync.landmarks.some((landmark) => landmark.id === id)) {
        throw new Error(`Manual video sync landmark id is duplicated: ${id}`)
      }
      const landmark = { id, type, videoSecond }
      if (type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) landmark.activitySecond = null
      validateLandmark(landmark, state.importedVideoDuration)
      if (state.manualVideoSync.landmarks.length >= VIDEO_SYNC_MAX_LANDMARKS) {
        throw new Error(`Manual video sync supports at most ${VIDEO_SYNC_MAX_LANDMARKS} landmarks`)
      }
      if (
        type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION &&
        state.manualVideoSync.landmarks.some((item) => item.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
      ) {
        throw new Error(`Manual video sync supports at most ${VIDEO_SYNC_MAX_LOCATION_LANDMARKS} location landmark`)
      }

      set((draft) => {
        draft.manualVideoSync.landmarks.push(landmark)
        if (type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) invalidateAllResults(draft)
        else invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
      })
      return id
    },

    moveVideoSyncLandmark: (id, videoSecond) => {
      validateLandmarkId(id)
      const state = get()
      const landmark = state.manualVideoSync.landmarks.find((item) => item.id === id)
      if (!landmark) throw new Error(`Manual video sync landmark was not found: ${id}`)
      validateVideoSecond(videoSecond, state.importedVideoDuration)

      set((draft) => {
        const target = draft.manualVideoSync.landmarks.find((item) => item.id === id)
        target.videoSecond = videoSecond
        if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) invalidateAllResults(draft)
        else invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
      })
    },

    setVideoSyncDetectedLocation: (activitySecond) => {
      validateActivitySecond(activitySecond)
      const detectedLocation = { id: VIDEO_SYNC_DETECTED_LOCATION_ID, type: VIDEO_SYNC_LANDMARK_TYPES.LOCATION, time: activitySecond }
      if (get().manualVideoSyncDetection?.location?.time === activitySecond) return
      set((draft) => {
        if (draft.manualVideoSyncDetection === null) draft.manualVideoSyncDetection = createEmptyVideoSyncDetection()
        draft.manualVideoSyncDetection.location = detectedLocation
        invalidateAllResults(draft)
      })
    },

    clearVideoSyncDetectedLocation: () => {
      const detection = get().manualVideoSyncDetection
      if (detection === null || detection.location === null) return
      set((draft) => {
        draft.manualVideoSyncDetection.location = null
        invalidateAllResults(draft)
      })
    },

    setVideoSyncLandmarkType: (id, type) => {
      validateLandmarkId(id)
      const state = get()
      const landmark = state.manualVideoSync.landmarks.find((item) => item.id === id)
      if (!landmark) throw new Error(`Manual video sync landmark was not found: ${id}`)

      const nextLandmark = { ...landmark, type }
      if (type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) nextLandmark.activitySecond = null
      else delete nextLandmark.activitySecond
      validateLandmark(nextLandmark, state.importedVideoDuration)

      if (
        type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION &&
        state.manualVideoSync.landmarks.some((item) => item.id !== id && item.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
      ) {
        throw new Error(`Manual video sync supports at most ${VIDEO_SYNC_MAX_LOCATION_LANDMARKS} location landmark`)
      }

      if (landmark.type === type) return

      set((draft) => {
        draft.manualVideoSync.landmarks = draft.manualVideoSync.landmarks.map((item) => (item.id === id ? nextLandmark : item))
        if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION || nextLandmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) {
          invalidateAllResults(draft)
        } else {
          invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
        }
      })
    },

    removeVideoSyncLandmark: (id) => {
      validateLandmarkId(id)
      const state = get()
      const landmark = state.manualVideoSync.landmarks.find((item) => item.id === id)
      if (!landmark) throw new Error(`Manual video sync landmark was not found: ${id}`)

      set((draft) => {
        draft.manualVideoSync.landmarks = draft.manualVideoSync.landmarks.filter((item) => item.id !== id)
        if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) invalidateAllResults(draft)
        else invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
      })
    },

    clearVideoSyncLandmarks: () => {
      if (!get().manualVideoSync.landmarks.length) return
      set((draft) => {
        draft.manualVideoSync.landmarks = []
        invalidateAllResults(draft)
      })
    },

    setVideoSyncSpeedThreshold: (value) => {
      const state = get()
      validateThreshold(value, VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH, 'Manual video sync speedThresholdKmh')
      if (state.manualVideoSync.speedThresholdKmh === value) return
      set((draft) => {
        draft.manualVideoSync.speedThresholdKmh = value
        invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
      })
    },

    setVideoSyncTurnThreshold: (value) => {
      const state = get()
      validateThreshold(value, VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES, 'Manual video sync turnThresholdDegrees')
      if (state.manualVideoSync.turnThresholdDegrees === value) return
      set((draft) => {
        draft.manualVideoSync.turnThresholdDegrees = value
        invalidateResult(draft, VIDEO_SYNC_MATCH_SCOPES.ALL)
      })
    },

    applyVideoSyncCandidate: (scope, candidate) => {
      requireMatchScope(scope)
      const state = get()
      const result = state.manualVideoSyncResults[scope]
      if (result.status !== MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH) {
        throw new Error('Cannot apply a stale manual video sync candidate')
      }
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate) || !Object.hasOwn(candidate, 'offset')) {
        throw new Error('Manual video sync candidate must include an offset')
      }
      if (!result.candidates.some((item) => item.offset === candidate.offset)) {
        throw new Error('Manual video sync candidate is not part of the current result')
      }

      state.setVideoSyncOffset(candidate.offset, { compensatePlayhead: true })
      state.setVideoSyncWarning(null)
    },

    beginVideoSyncCalculation: (scope) => {
      requireMatchScope(scope)
      const state = get()
      if (Object.values(state.manualVideoSyncResults).some((result) => result.status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING)) {
        return null
      }
      const revision = state.manualVideoSyncResults[scope].revision
      set((draft) => {
        const result = draft.manualVideoSyncResults[scope]
        result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING
        result.error = null
      })
      return revision
    },

    completeVideoSyncCalculation: (scope, revision, calculation) => {
      requireMatchScope(scope)
      requireRevision(revision)
      requireCalculationResult(calculation)
      const currentResult = get().manualVideoSyncResults[scope]
      if (currentResult.revision !== revision || currentResult.status !== MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING) return false
      set((draft) => {
        const result = draft.manualVideoSyncResults[scope]
        draft.manualVideoSyncDetection = structuredClone(calculation.detection)
        result.candidates = structuredClone(calculation.candidates)
        result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH
        result.error = null
        result.hasSearched = true
      })
      return true
    },

    completeVideoSyncDetection: (detection) => {
      set((draft) => {
        draft.manualVideoSyncDetection = structuredClone(detection)
      })
      return true
    },

    failVideoSyncCalculation: (scope, revision, message) => {
      requireMatchScope(scope)
      requireRevision(revision)
      requireCalculationError(message)
      const currentResult = get().manualVideoSyncResults[scope]
      if (currentResult.revision !== revision || currentResult.status !== MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING) return false
      set((draft) => {
        const result = draft.manualVideoSyncResults[scope]
        result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.ERROR
        result.error = message
        result.hasSearched = true
      })
      return true
    },

    failVideoSyncDetection: (message) => {
      requireCalculationError(message)
      set((draft) => {
        const result = draft.manualVideoSyncResults[VIDEO_SYNC_MATCH_SCOPES.ALL]
        result.status = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.ERROR
        result.error = message
      })
      return true
    },

    clearVideoSyncForVideo: () =>
      set((draft) => {
        draft.manualVideoSync.landmarks = []
        resetDerivedState(draft)
      }),

    clearVideoSyncForActivity: () =>
      set((draft) => {
        resetDerivedState(draft)
      }),

    // Project data is validated once by Rust before this canonical state reaches the store.
    hydrateVideoSyncState: (manualState) => {
      set((draft) => {
        draft.manualVideoSync = cloneManualState(manualState)
        resetDerivedState(draft)
      })
    },

    resetVideoSyncState: () => {
      set((draft) => {
        draft.manualVideoSync = createDefaultManualState()
        resetDerivedState(draft)
      })
    },
  }
}
