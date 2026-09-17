import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { VIDEO_SYNC_LANDMARK_TYPES } from '@/features/video-sync/data/videoSyncConstants'
import useStore from '@/store/useStore'

describe('manual video sync store contract', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({ importedVideoDuration: 60 })
    let nextId = 0
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => `landmark-${++nextId}`),
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  test('creates typed landmarks and enforces total and location limits', () => {
    const state = useStore.getState()
    const stopId = state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 4)
    state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN, 8)
    state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, 12)
    state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.LOCATION, 16)

    expect(useStore.getState().manualVideoSync.landmarks).toEqual([
      { id: stopId, type: 'stop', videoSecond: 4 },
      { id: 'landmark-2', type: 'leftTurn', videoSecond: 8 },
      { id: 'landmark-3', type: 'rightTurn', videoSecond: 12 },
      { id: 'landmark-4', type: 'location', videoSecond: 16, activitySecond: null },
    ])
    expect(() => state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.LOCATION, 20)).toThrow(/at most 1 location/)
    state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 20)
    expect(() => state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, 24)).toThrow(/at most 5 landmarks/)
  })

  test('moves, removes, and clears landmarks through domain actions', () => {
    const state = useStore.getState()
    const id = state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 4)

    state.moveVideoSyncLandmark(id, 10)
    expect(useStore.getState().manualVideoSync.landmarks[0].videoSecond).toBe(10)

    state.removeVideoSyncLandmark(id)
    expect(useStore.getState().manualVideoSync.landmarks).toEqual([])

    const secondId = state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, 20)
    expect(secondId).toBe('landmark-2')
    state.clearVideoSyncLandmarks()
    expect(useStore.getState().manualVideoSync.landmarks).toEqual([])
  })

  test('changes landmark types while preserving canonical shapes and location limits', () => {
    const state = useStore.getState()
    const landmarkId = state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 4)

    state.setVideoSyncLandmarkType(landmarkId, VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN)
    expect(useStore.getState().manualVideoSync.landmarks[0]).toEqual({ id: landmarkId, type: 'leftTurn', videoSecond: 4 })

    state.setVideoSyncLandmarkType(landmarkId, VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
    expect(useStore.getState().manualVideoSync.landmarks[0]).toEqual({
      id: landmarkId,
      type: 'location',
      videoSecond: 4,
      activitySecond: null,
    })

    const secondId = state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, 8)
    expect(() => state.setVideoSyncLandmarkType(secondId, VIDEO_SYNC_LANDMARK_TYPES.LOCATION)).toThrow(/at most 1 location/)

    state.setVideoSyncLandmarkType(landmarkId, VIDEO_SYNC_LANDMARK_TYPES.STOP)
    expect(useStore.getState().manualVideoSync.landmarks[0]).toEqual({ id: landmarkId, type: 'stop', videoSecond: 4 })
  })

  test('rejects malformed action input at the store boundary', () => {
    const state = useStore.getState()

    expect(() => state.addVideoSyncLandmark('turn', 4)).toThrow(/Unsupported manual video sync landmark type/)
    expect(() => state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, Number.NaN)).toThrow(/finite number/)
    expect(() => state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 61)).toThrow(/within the imported video duration/)
    expect(() => state.setVideoSyncSpeedThreshold(0)).toThrow(/between 1 and 10/)
    expect(() => state.setVideoSyncTurnThreshold(361)).toThrow(/between 90 and 360/)
    expect(() => state.moveVideoSyncLandmark('missing-id', 4)).toThrow(/was not found/)
  })

  test('discards calculation results after the input revision changes', () => {
    const state = useStore.getState()
    const revision = state.beginVideoSyncCalculation()
    state.addVideoSyncLandmark(VIDEO_SYNC_LANDMARK_TYPES.STOP, 4)

    expect(state.completeVideoSyncCalculation(revision, { detection: {}, candidates: [] })).toBe(false)
    expect(useStore.getState().manualVideoSyncCandidateStatus).toBe('idle')
    expect(useStore.getState().manualVideoSyncCandidates).toEqual([])
  })

  test('keeps activity-owned manual state but clears its derived calculation state', () => {
    const state = useStore.getState()
    state.hydrateVideoSyncState({
      landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 4 }],
      speedThresholdKmh: 7,
      turnThresholdDegrees: 120,
    })
    const revision = state.beginVideoSyncCalculation()
    state.completeVideoSyncCalculation(revision, { detection: { stops: ['old'] }, candidates: [{ offset: 4 }] })

    state.clearVideoSyncForActivity()

    const afterActivityReset = useStore.getState()
    expect(afterActivityReset.manualVideoSync).toEqual({
      landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 4 }],
      speedThresholdKmh: 7,
      turnThresholdDegrees: 120,
    })
    expect(afterActivityReset.manualVideoSyncDetection).toBeNull()
    expect(afterActivityReset.manualVideoSyncCandidates).toEqual([])
    expect(afterActivityReset.manualVideoSyncCandidateStatus).toBe('idle')

    afterActivityReset.clearVideoSyncForVideo()

    const afterVideoReset = useStore.getState()
    expect(afterVideoReset.manualVideoSync.landmarks).toEqual([])
    expect(afterVideoReset.manualVideoSync.speedThresholdKmh).toBe(7)
    expect(afterVideoReset.manualVideoSync.turnThresholdDegrees).toBe(120)
  })

  test('blocks stale candidates and compensates an out-of-video playhead atomically', () => {
    useStore.setState({
      activitySummary: { durationSeconds: 100 },
      importedVideoPath: 'C:\\video.mp4',
      importedVideoDuration: 20,
      selectedSecond: 80,
      videoSyncOffsetSeconds: 0,
      manualVideoSyncCandidateStatus: 'fresh',
      manualVideoSyncCandidates: [{ offset: 10 }],
    })

    let updateCount = 0
    const unsubscribe = useStore.subscribe(() => {
      updateCount += 1
    })
    useStore.getState().applyVideoSyncCandidate({ offset: 10 })
    unsubscribe()

    expect(updateCount).toBe(1)
    expect(useStore.getState().videoSyncOffsetSeconds).toBe(10)
    expect(useStore.getState().selectedSecond).toBe(90)

    useStore.setState({
      manualVideoSyncCandidateStatus: 'stale',
      videoSyncOffsetSeconds: 0,
      selectedSecond: 80,
    })
    expect(() => useStore.getState().applyVideoSyncCandidate({ offset: 10 })).toThrow(/stale/)
    expect(useStore.getState().videoSyncOffsetSeconds).toBe(0)
    expect(useStore.getState().selectedSecond).toBe(80)
  })
})
