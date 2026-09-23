import { act, renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'
import useVideoSyncCalculation from '@/features/video-sync/hooks/useVideoSyncCalculation'

const LANDMARKS = [
  { id: 'video-stop-1', type: 'stop', videoSecond: 4 },
  { id: 'video-stop-2', type: 'stop', videoSecond: 12 },
]

function createActivity(speed = [3, 3, 3, 3, 3]) {
  return {
    sample_elapsed_seconds: speed.map((_, index) => index),
    speed,
    heading: speed.map(() => 0),
    sample_course_points: speed.map(() => [50, 14]),
  }
}

function resetStore(activity = createActivity()) {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({
    importedVideoPath: 'C:\\video.mp4',
    importedVideoDuration: 30,
    manualVideoSync: {
      landmarks: structuredClone(LANDMARKS),
      detectedLocationSecond: null,
      speedThresholdKmh: 5,
      turnThresholdDegrees: 90,
    },
    parsedActivity: activity,
  })
}

async function waitForDetection(result) {
  await waitFor(() => expect(result.current.isCalculating).toBe(false))
  expect(useStore.getState().manualVideoSyncDetection).not.toBeNull()
}

describe('manual video-sync calculation orchestration', () => {
  test('preserves the location event when automatic detection refreshes the canonical detection model', async () => {
    resetStore()
    useStore.getState().setVideoSyncDetectedLocation(2)
    const { result } = renderHook(() => useVideoSyncCalculation())

    await waitForDetection(result)

    expect(useStore.getState().manualVideoSyncDetection.location).toEqual({ id: 'detected-course-location', type: 'location', time: 2 })
  })

  test('gates the first candidate search on usable landmark telemetry', async () => {
    resetStore(createActivity([null, null, null, null, null]))
    const { result } = renderHook(() => useVideoSyncCalculation())

    await waitForDetection(result)
    expect(result.current.eligibility.canCalculateAll).toBe(false)

    let calculated
    await act(async () => {
      calculated = await result.current.calculate('all')
    })

    expect(calculated).toBe(false)
    expect(useStore.getState().manualVideoSyncResults.all).toMatchObject({ hasSearched: false, status: 'idle' })
  })

  test('reruns detection and matching after a committed sensitivity change', async () => {
    resetStore()
    const { result } = renderHook(() => useVideoSyncCalculation())

    await waitForDetection(result)
    await act(async () => {
      expect(await result.current.calculate('all')).toBe(true)
    })
    expect(useStore.getState().manualVideoSyncResults.all.hasSearched).toBe(true)

    const previousRevision = useStore.getState().manualVideoSyncResults.all.revision
    act(() => useStore.getState().setVideoSyncSpeedThreshold(6))

    await waitFor(() => {
      expect(useStore.getState().manualVideoSyncResults.all).toMatchObject({ revision: previousRevision + 1, status: 'fresh' })
    })
    expect(useStore.getState().manualVideoSyncResults.all.hasSearched).toBe(true)
  })

  test('discards a result when a landmark changes while calculation is pending', async () => {
    resetStore()
    const { result } = renderHook(() => useVideoSyncCalculation())

    await waitForDetection(result)
    await act(async () => {
      expect(await result.current.calculate('all')).toBe(true)
    })

    const previousDetection = useStore.getState().manualVideoSyncDetection
    let pendingCalculation
    act(() => {
      pendingCalculation = result.current.calculate('all')
      useStore.getState().moveVideoSyncLandmark('video-stop-2', 13)
    })

    await act(async () => {
      expect(await pendingCalculation).toBe(false)
    })

    expect(useStore.getState().manualVideoSyncResults.all.status).toBe('stale')
    expect(useStore.getState().manualVideoSyncDetection).toBe(previousDetection)
  })

  test('stores and applies a location-only result through the canonical candidate workflow', async () => {
    resetStore()
    useStore.setState((state) => ({
      manualVideoSync: {
        ...state.manualVideoSync,
        landmarks: [{ id: 'video-location', type: 'location', videoSecond: 4, activitySecond: null }],
      },
    }))
    useStore.getState().setVideoSyncDetectedLocation(12)
    const { result } = renderHook(() => useVideoSyncCalculation())

    await waitForDetection(result)
    await act(async () => {
      expect(await result.current.calculate('location')).toBe(true)
    })

    const candidate = useStore.getState().manualVideoSyncResults.location.candidates[0]
    expect(candidate).toMatchObject({ variant: 'locationOnly', offset: 8, matchedCount: 1, eligibleCount: 1 })

    act(() => useStore.getState().applyVideoSyncCandidate('location', candidate))
    expect(useStore.getState().videoSyncOffsetSeconds).toBe(8)
  })
})
