import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES } from '@/features/video-sync/data/videoSyncConstants'
import useVideoSyncWorkspace from '@/features/video-sync/hooks/useVideoSyncWorkspace'
import { resolveVideoSyncMarkControls } from '@/features/video-sync/utils/videoSyncPresentation'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/features/video-sync/hooks/useVideoSyncCalculation', () => ({
  default: () => ({
    calculate: vi.fn(),
    eligibility: {
      canCalculateAll: false,
      canMatchAll: false,
      canCalculateLocation: false,
      allExplanation: 'At least two landmarks are required',
    },
    isCalculating: false,
  }),
}))

const videoSync = {}

function createToolbarDrawer(overrides = {}) {
  return {
    activeTool: VIDEO_SYNC_TOOL,
    visible: true,
    renderDrawerContent: true,
    ...overrides,
  }
}

describe('useVideoSyncWorkspace', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      ...videoStoreState([createTestVideo({ path: 'C:\\video.mp4', durationSeconds: 20 })], { 'video-1': 2 }),
      selectedSecond: 8,
      manualVideoSync: {
        videoId: null,
        landmarks: [],
        detectedLocationSecond: null,
        speedThresholdKmh: 5,
        turnThresholdDegrees: 180,
      },
    })
    vi.stubGlobal('crypto', { randomUUID: vi.fn(() => `landmark-${useStore.getState().manualVideoSync.landmarks.length + 1}`) })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  test('preserves the selected workspace when its unpinned drawer closes', () => {
    const configBefore = useStore.getState().config
    const { result } = renderHook(() => {
      const toolbarDrawer = useStore(
        useShallow((state) => ({
          activeTool: state.activeLeftDrawerTool,
          visible: state.leftDrawerVisible,
        })),
      )
      return useVideoSyncWorkspace({ toolbarDrawer, videoSync })
    })

    expect(result.current.videoSyncMode).toBe(false)

    act(() => useStore.getState().selectLeftDrawerTool(VIDEO_SYNC_TOOL))
    expect(result.current.videoSyncMode).toBe(true)

    act(() => useStore.getState().selectLeftDrawerTool(VIDEO_SYNC_TOOL))

    expect(result.current.videoSyncMode).toBe(true)
    expect(useStore.getState().config).toBe(configBefore)
  })

  test('marks typed observations at the latest video-local time without subscribing to playhead updates', () => {
    let renderCount = 0
    const { result } = renderHook(() => {
      renderCount += 1
      return useVideoSyncWorkspace({ toolbarDrawer: createToolbarDrawer(), videoSync })
    })

    act(() => {
      result.current.markControls.onMarkStop()
      result.current.markControls.onMarkLeftTurn()
      result.current.markControls.onMarkRightTurn()
      result.current.markControls.onMarkLocation()
    })

    expect(useStore.getState().manualVideoSync.landmarks).toEqual([
      { id: 'landmark-1', type: VIDEO_SYNC_LANDMARK_TYPES.STOP, videoSecond: 6 },
      { id: 'landmark-2', type: VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN, videoSecond: 6 },
      { id: 'landmark-3', type: VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN, videoSecond: 6 },
      { id: 'landmark-4', type: VIDEO_SYNC_LANDMARK_TYPES.LOCATION, videoSecond: 6, activitySecond: null },
    ])

    const markStop = result.current.markControls.onMarkStop
    act(() => useStore.getState().clearVideoSyncLandmarks())
    const renderCountBeforePlayback = renderCount
    act(() => useStore.getState().setSelectedSecond(9))

    expect(renderCount).toBe(renderCountBeforePlayback)
    act(() => markStop())
    expect(useStore.getState().manualVideoSync.landmarks).toEqual([{ id: 'landmark-1', type: VIDEO_SYNC_LANDMARK_TYPES.STOP, videoSecond: 7 }])
  })

  test('marks landmarks in the selected clip, else the clip at the playhead, which then holds the sync for every clip', () => {
    useStore.setState({
      ...videoStoreState(
        [
          createTestVideo({ id: 'video-1', path: 'C:\\first.mp4', durationSeconds: 20 }),
          createTestVideo({ id: 'video-2', path: 'C:\\second.mp4', durationSeconds: 10, importOrder: 1 }),
        ],
        { 'video-1': 2, 'video-2': 30 },
      ),
      selectedSecond: 33,
    })
    const { result } = renderHook(() => useVideoSyncWorkspace({ toolbarDrawer: createToolbarDrawer(), videoSync }))

    expect(result.current.drawer).toMatchObject({ appliedOffset: 0, hasVideos: true, syncHeldBy: null })
    expect(result.current.markControls).toMatchObject({ isHeldByOtherVideo: false, videoDurationSeconds: 10, videoOffsetSeconds: 30 })

    act(() => result.current.markControls.onMarkStop())

    expect(useStore.getState().manualVideoSync).toMatchObject({
      videoId: 'video-2',
      landmarks: [{ id: 'landmark-1', type: VIDEO_SYNC_LANDMARK_TYPES.STOP, videoSecond: 3 }],
    })
    expect(result.current.drawer.appliedOffset).toBe(30)

    act(() => useStore.getState().setSelectedSecond(8))
    expect(result.current.drawer.syncHeldBy).toBe('second.mp4')
    expect(result.current.markControls).toMatchObject({ isHeldByOtherVideo: true, heldByOtherVideoReason: 'Landmarks are marked in second.mp4' })

    act(() => useStore.getState().selectClip({ laneId: 'video:video-2', segmentKey: 'start' }))
    expect(result.current.drawer.syncHeldBy).toBeNull()
    expect(result.current.markControls).toMatchObject({ isHeldByOtherVideo: false, videoOffsetSeconds: 30 })

    act(() => useStore.getState().selectClip({ laneId: 'video:video-1', segmentKey: null }))
    expect(result.current.drawer.syncHeldBy).toBe('second.mp4')

    act(() => result.current.drawer.onShowSyncVideo())
    expect(useStore.getState()).toMatchObject({ selectedSecond: 30, selectedClip: { laneId: 'video:video-2', segmentKey: null } })
    expect(result.current.drawer.syncHeldBy).toBeNull()
  })

  test('keeps sensitivity drafts local until commit and scrubs landmarks through the applied offset', () => {
    const { result } = renderHook(() => useVideoSyncWorkspace({ toolbarDrawer: createToolbarDrawer(), videoSync }))

    act(() => result.current.drawer.onSpeedThresholdChange(10))
    expect(useStore.getState().manualVideoSync.speedThresholdKmh).toBe(5)

    act(() => result.current.drawer.onSpeedThresholdCommit(10))
    expect(useStore.getState().manualVideoSync.speedThresholdKmh).toBe(10)

    act(() => result.current.drawer.onTurnThresholdCommit(180))
    expect(useStore.getState().manualVideoSync.turnThresholdDegrees).toBe(180)
  })

  test('resolves mark controls for the current playhead', () => {
    const markControls = {
      hasLandmarkCapacity: true,
      hasLocationLandmark: false,
      hasVideos: true,
      heldByOtherVideoReason: null,
      isHeldByOtherVideo: false,
      videoDurationSeconds: 10,
      landmarkLimitReason: 'Landmark limit',
      locationLimitReason: 'Location limit',
      onMarkLeftTurn: vi.fn(),
      onMarkLocation: vi.fn(),
      onMarkRightTurn: vi.fn(),
      onMarkStop: vi.fn(),
      playheadOutsideVideoReason: 'Outside video',
      videoRequiredReason: 'Video required',
      videoOffsetSeconds: 0,
    }

    expect(resolveVideoSyncMarkControls(markControls, 0.5, true)).toMatchObject({
      canMark: true,
      canMarkLocation: true,
      markDisabledReason: null,
    })
    expect(resolveVideoSyncMarkControls({ ...markControls, hasVideos: false, videoDurationSeconds: null }, 0.5, true)).toMatchObject({
      canMark: false,
      markDisabledReason: 'Video required',
    })
    expect(resolveVideoSyncMarkControls({ ...markControls, videoDurationSeconds: null }, 0.5, true)).toMatchObject({
      canMark: false,
      markDisabledReason: 'Outside video',
    })
    expect(
      resolveVideoSyncMarkControls(
        { ...markControls, isHeldByOtherVideo: true, heldByOtherVideoReason: 'Other video', hasLocationLandmark: true },
        0.5,
        true,
      ),
    ).toMatchObject({ canMark: false, canMarkLocation: false, markDisabledReason: 'Other video', locationDisabledReason: 'Other video' })
    expect(resolveVideoSyncMarkControls(markControls, 0.5, false)).toBeNull()
  })
})
