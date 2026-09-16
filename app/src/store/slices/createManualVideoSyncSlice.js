import {
  MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES,
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_MAX_LANDMARKS,
  VIDEO_SYNC_MAX_LOCATION_LANDMARKS,
  VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH,
  VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES,
} from '@/features/video-sync/data/videoSyncConstants'
import {
  cloneManualState,
  createDefaultManualState,
  validateLandmarkId,
  validateLandmark,
  validateThreshold,
  validateVideoSecond,
} from '@/features/video-sync/utils/manualVideoSyncContract'

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

function invalidateDerivedState(draft, { clearCandidates = false } = {}) {
  draft.manualVideoSyncInputRevision += 1
  if (clearCandidates) {
    draft.manualVideoSyncCandidates = []
    draft.manualVideoSyncDetection = null
    draft.manualVideoSyncHasSearched = false
    draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE
  } else if (draft.manualVideoSyncHasSearched) {
    draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.STALE
  } else if (draft.manualVideoSyncCandidateStatus === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING) {
    draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE
  }
  draft.manualVideoSyncError = null
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
    manualVideoSyncCandidates: [],
    manualVideoSyncCandidateStatus: MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE,
    manualVideoSyncError: null,
    manualVideoSyncInputRevision: 0,
    manualVideoSyncHasSearched: false,

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
        invalidateDerivedState(draft)
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
        invalidateDerivedState(draft)
      })
    },

    removeVideoSyncLandmark: (id) => {
      validateLandmarkId(id)
      const state = get()
      if (!state.manualVideoSync.landmarks.some((landmark) => landmark.id === id)) {
        throw new Error(`Manual video sync landmark was not found: ${id}`)
      }

      set((draft) => {
        draft.manualVideoSync.landmarks = draft.manualVideoSync.landmarks.filter((landmark) => landmark.id !== id)
        invalidateDerivedState(draft)
      })
    },

    clearVideoSyncLandmarks: () => {
      if (!get().manualVideoSync.landmarks.length) return
      set((draft) => {
        draft.manualVideoSync.landmarks = []
        invalidateDerivedState(draft)
      })
    },

    setVideoSyncSpeedThreshold: (value) => {
      const state = get()
      validateThreshold(value, VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH, 'Manual video sync speedThresholdKmh')
      if (state.manualVideoSync.speedThresholdKmh === value) return
      set((draft) => {
        draft.manualVideoSync.speedThresholdKmh = value
        invalidateDerivedState(draft)
      })
    },

    setVideoSyncTurnThreshold: (value) => {
      const state = get()
      validateThreshold(value, VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES, 'Manual video sync turnThresholdDegrees')
      if (state.manualVideoSync.turnThresholdDegrees === value) return
      set((draft) => {
        draft.manualVideoSync.turnThresholdDegrees = value
        invalidateDerivedState(draft)
      })
    },

    applyVideoSyncCandidate: (candidate) => {
      const state = get()
      if (state.manualVideoSyncCandidateStatus !== MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH) {
        throw new Error('Cannot apply a stale manual video sync candidate')
      }
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate) || !Object.hasOwn(candidate, 'offset')) {
        throw new Error('Manual video sync candidate must include an offset')
      }
      if (!state.manualVideoSyncCandidates.some((item) => item.offset === candidate.offset)) {
        throw new Error('Manual video sync candidate is not part of the current result')
      }

      state.setVideoSyncOffset(candidate.offset, { compensatePlayhead: true })
    },

    beginVideoSyncCalculation: () => {
      if (get().manualVideoSyncCandidateStatus === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING) return null
      const revision = get().manualVideoSyncInputRevision
      set((draft) => {
        draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.CALCULATING
        draft.manualVideoSyncError = null
      })
      return revision
    },

    completeVideoSyncCalculation: (revision, result) => {
      requireRevision(revision)
      requireCalculationResult(result)
      if (get().manualVideoSyncInputRevision !== revision) return false
      set((draft) => {
        draft.manualVideoSyncDetection = structuredClone(result.detection)
        draft.manualVideoSyncCandidates = structuredClone(result.candidates)
        draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH
        draft.manualVideoSyncError = null
        draft.manualVideoSyncHasSearched = true
      })
      return true
    },

    completeVideoSyncDetection: (revision, detection) => {
      requireRevision(revision)
      if (get().manualVideoSyncInputRevision !== revision) return false
      set((draft) => {
        draft.manualVideoSyncDetection = structuredClone(detection)
        draft.manualVideoSyncCandidateStatus = draft.manualVideoSyncHasSearched
          ? MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.STALE
          : MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE
        draft.manualVideoSyncError = null
      })
      return true
    },

    failVideoSyncCalculation: (revision, message) => {
      requireRevision(revision)
      requireCalculationError(message)
      if (get().manualVideoSyncInputRevision !== revision) return false
      set((draft) => {
        draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.ERROR
        draft.manualVideoSyncError = message
        draft.manualVideoSyncHasSearched = true
      })
      return true
    },

    failVideoSyncDetection: (revision, message) => {
      requireRevision(revision)
      requireCalculationError(message)
      if (get().manualVideoSyncInputRevision !== revision) return false
      set((draft) => {
        draft.manualVideoSyncCandidateStatus = MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.ERROR
        draft.manualVideoSyncError = message
      })
      return true
    },

    invalidateVideoSyncCalculation: () =>
      set((draft) => {
        invalidateDerivedState(draft)
      }),

    clearVideoSyncForVideo: () =>
      set((draft) => {
        draft.manualVideoSync.landmarks = []
        invalidateDerivedState(draft, { clearCandidates: true })
      }),

    clearVideoSyncForActivity: () =>
      set((draft) => {
        invalidateDerivedState(draft, { clearCandidates: true })
      }),

    // Project data is validated once by Rust before this canonical state reaches the store.
    hydrateVideoSyncState: (manualState) => {
      set((draft) => {
        draft.manualVideoSync = cloneManualState(manualState)
        invalidateDerivedState(draft, { clearCandidates: true })
      })
    },

    resetVideoSyncState: () => {
      set((draft) => {
        draft.manualVideoSync = createDefaultManualState()
        invalidateDerivedState(draft, { clearCandidates: true })
      })
    },
  }
}
