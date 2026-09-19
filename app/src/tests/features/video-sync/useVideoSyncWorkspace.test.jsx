import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { VIDEO_SYNC_TOOL } from '@/store/slices/createLayoutSlice'
import { VIDEO_SYNC_LANDMARK_TYPES } from '@/features/video-sync/data/videoSyncConstants'
import useVideoSyncWorkspace, { resolveVideoSyncMarkControls } from '@/features/video-sync/hooks/useVideoSyncWorkspace'

vi.mock('@/features/video-sync/hooks/useVideoSyncCalculation', () => ({
  default: () => ({
    calculate: vi.fn(),
    eligibility: {
      canCalculate: false,
      canMatchLandmarks: false,
      canUseMapOnly: false,
      explanation: 'At least two landmarks are required',
    },
    isCalculating: false,
  }),
}))

const videoSummary = { path: 'C:\\video.mp4', timeSource: 'ffprobe' }
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
      importedVideoDuration: 20,
      selectedSecond: 8,
      videoSyncOffsetSeconds: 2,
      manualVideoSync: {
        landmarks: [],
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
      return useVideoSyncWorkspace({ toolbarDrawer, videoSummary, videoSync })
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
      return useVideoSyncWorkspace({ toolbarDrawer: createToolbarDrawer(), videoSummary, videoSync })
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

  test('keeps sensitivity drafts local until commit and scrubs landmarks through the applied offset', () => {
    const { result } = renderHook(() => useVideoSyncWorkspace({ toolbarDrawer: createToolbarDrawer(), videoSummary, videoSync }))

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
      importedVideoDuration: 10,
      landmarkLimitReason: 'Landmark limit',
      locationLimitReason: 'Location limit',
      onMarkLeftTurn: vi.fn(),
      onMarkLocation: vi.fn(),
      onMarkRightTurn: vi.fn(),
      onMarkStop: vi.fn(),
      playheadOutsideVideoReason: 'Outside video',
      videoRequiredReason: 'Video required',
      videoSyncOffsetSeconds: 0,
    }

    expect(resolveVideoSyncMarkControls(markControls, 0.5, true)).toMatchObject({
      canMark: true,
      canMarkLocation: true,
      markDisabledReason: null,
    })
    expect(resolveVideoSyncMarkControls({ ...markControls, importedVideoDuration: null }, 0.5, true)).toMatchObject({
      canMark: false,
      markDisabledReason: 'Video required',
    })
    expect(resolveVideoSyncMarkControls(markControls, 0.5, false)).toBeNull()
  })
})
