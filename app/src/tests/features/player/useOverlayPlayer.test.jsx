import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useOverlayPlayer from '@/features/player/hooks/useOverlayPlayer'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function createTimelineElement(width = 500) {
  return {
    getBoundingClientRect: () => ({
      left: 0,
      width,
    }),
  }
}

function createPointerEvent(clientX) {
  return {
    button: 0,
    clientX,
    currentTarget: {
      focus: vi.fn(),
      releasePointerCapture: vi.fn(),
      setPointerCapture: vi.fn(),
    },
    pointerId: 1,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  }
}

function resetStore({
  videos = [createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })],
  offsets = { 'video-1': 10 },
  ...overrides
} = {}) {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({
    activitySource: { kind: 'file', path: 'C:\\activities\\ride.fit' },
    activitySummary: {
      availableMetrics: [],
      durationSeconds: 100,
      fileFormat: 'fit',
      fileName: 'activity.fit',
    },
    fallbackDurationSeconds: 73,
    selectedSecond: 0,
    ...videoStoreState(videos, offsets),
    ...overrides,
  })
}

function createTwoClipStore() {
  resetStore({
    videos: [
      createTestVideo({ id: 'first', path: '/videos/first.mp4', durationSeconds: 20 }),
      createTestVideo({ id: 'second', path: '/videos/second.mp4', durationSeconds: 20, importOrder: 1 }),
    ],
    offsets: { first: 10, second: 50 },
  })
}

function timelineClips(result) {
  return result.current.timeline.lanes.flatMap((lane) => lane.clips)
}

function findLane(result, clipId) {
  return timelineClips(result).find((clip) => clip.id === clipId)
}

function dragLane(result, laneId, fromClientX, toClientX) {
  act(() => {
    findLane(result, laneId).clipProps.onPointerDown(createPointerEvent(fromClientX))
  })
  act(() => {
    findLane(result, laneId).clipProps.onPointerMove(createPointerEvent(toClientX))
  })
  act(() => {
    findLane(result, laneId).clipProps.onPointerUp(createPointerEvent(toClientX))
  })
}

describe('useOverlayPlayer', () => {
  beforeEach(() => {
    resetStore()
  })

  test('builds the top-level toolbar and timeline view models', () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    expect(result.current.isVisible).toBe(true)
    expect(result.current.toolbar.fitTargets.map((target) => target.id)).toEqual(['video', 'activity'])
    expect(timelineClips(result).map((clip) => clip.id)).toEqual(['video:video-1', 'activity'])
  })

  test('routes toolbar transport commands through store playback state', () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    act(() => {
      result.current.toolbar.transport.stepForward()
    })

    expect(useStore.getState().selectedSecond).toBe(1)

    act(() => {
      result.current.toolbar.transport.jumpToEnd()
    })

    expect(useStore.getState().selectedSecond).toBe(100)
    expect(useStore.getState().previewPlaybackState).toBe('paused')
  })

  test('routes playback, export, mute, and synchronization shortcuts', () => {
    const { unmount } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
    })
    expect(useStore.getState().selectedSecond).toBe(1)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home' }))
    })
    expect(useStore.getState().selectedSecond).toBe(0)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    })
    expect(useStore.getState().renderSettings.range).toMatchObject({ from: 0, type: 'custom' })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm' }))
    })
    expect(useStore.getState().isVideoMuted).toBe(true)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: ']' }))
    })
    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(11)

    unmount()
  })

  test('measures timeline after mount, shows clip geometry, and drags the playhead by timeline coordinates', async () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    expect(result.current.timeline.widthPx).toBe(0)
    expect(timelineClips(result)[0].isVisible).toBe(false)

    act(() => {
      result.current.timeline.containerProps.ref(createTimelineElement(500))
    })

    expect(result.current.timeline.widthPx).toBe(500)
    expect(timelineClips(result)[0].isVisible).toBe(true)
    expect(timelineClips(result)[0].clipStyle.width).not.toBe('0%')

    act(() => {
      result.current.timeline.playhead.handleProps.onPointerDown({
        button: 0,
        clientX: 250,
        currentTarget: {
          hasPointerCapture: () => true,
          releasePointerCapture: () => {},
          setPointerCapture: () => {},
        },
        pointerId: 1,
        stopPropagation: () => {},
      })
    })

    await act(async () => {
      await new Promise((resolve) => window.requestAnimationFrame(resolve))
    })

    expect(useStore.getState().selectedSecond).toBe(50)
    expect(result.current.timeline.playhead.style.left).toBe(250)
  })

  test('hides the complete export-range presentation in video-sync mode', () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black', videoSyncMode: true }))

    act(() => {
      result.current.timeline.containerProps.ref(createTimelineElement(500))
      result.current.toolbar.exportRange.setStart()
    })

    expect(result.current.toolbar.exportRange.isCustom).toBe(true)
    expect(result.current.timeline.exportMarkers).toEqual([])
    expect(timelineClips(result).every((clip) => clip.highlightStyle === null)).toBe(true)
  })

  test('puts every clip on the video lane and fits the video target to every clip', () => {
    createTwoClipStore()
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    expect(result.current.timeline.lanes.map((lane) => lane.id)).toEqual(['video', 'activity'])
    expect(timelineClips(result).map((clip) => clip.id)).toEqual(['video:first', 'video:second', 'activity'])
    expect(result.current.toolbar.fitTargets.map((target) => target.id)).toEqual(['video', 'activity'])

    act(() => {
      result.current.toolbar.fitTargets.find((target) => target.id === 'video').onSelect()
    })

    expect(useStore.getState().timelineViewport.viewStart).toBeLessThan(10)
    expect(useStore.getState().timelineViewport.viewEnd).toBeGreaterThan(70)
    expect(useStore.getState().timelineViewport.viewEnd).toBeLessThan(100)
  })

  test('commits a video-lane drag to that clip only', () => {
    createTwoClipStore()
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    act(() => {
      result.current.timeline.containerProps.ref(createTimelineElement(500))
    })
    dragLane(result, 'video:second', 300, 350)

    expect(useStore.getState().videoEdits.first.offsetSeconds).toBe(10)
    expect(useStore.getState().videoEdits.second.offsetSeconds).toBe(60)
    expect(useStore.getState().videoDragPreview).toBeNull()
  })

  test('commits an activity-lane drag as an opposite shift of every clip', () => {
    createTwoClipStore()
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    act(() => {
      result.current.timeline.containerProps.ref(createTimelineElement(500))
    })
    dragLane(result, 'activity', 250, 275)

    expect(useStore.getState().videoEdits.first.offsetSeconds).toBe(5)
    expect(useStore.getState().videoEdits.second.offsetSeconds).toBe(45)
    expect(useStore.getState().videoDragPreview).toBeNull()
  })

  test('shifts every clip with the global synchronization nudge', () => {
    createTwoClipStore()
    const { unmount } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: ']' }))
    })

    expect(useStore.getState().videoEdits.first.offsetSeconds).toBe(11)
    expect(useStore.getState().videoEdits.second.offsetSeconds).toBe(51)

    unmount()
  })

  test('marks a deleted part between Shift+I and Shift+O on the clip under the playhead', () => {
    useStore.setState({ selectedSecond: 15 })
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    expect(result.current.toolbar.deletedPart).toMatchObject({ isVisible: true, canStart: true, canEnd: false, markLabel: null })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'I', shiftKey: true }))
    })
    expect(useStore.getState().deletedPartMark).toEqual({ videoId: 'video-1', sourceSecond: 5 })
    expect(result.current.toolbar.deletedPart.markLabel).toBe('ride.mp4 00:05')
    expect(findLane(result, 'video:video-1').markStyle).toEqual({ left: '25%' })

    act(() => useStore.getState().setSelectedSecond(22))
    expect(result.current.toolbar.deletedPart.canEnd).toBe(true)
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'O', shiftKey: true }))
    })
    expect(useStore.getState().videoEdits['video-1'].deletedParts).toEqual([
      expect.objectContaining({ fromSecond: 5, toSecond: 12, transition: null }),
    ])
    expect(useStore.getState().deletedPartMark).toBeNull()
    expect(findLane(result, 'video:video-1').cuts.map((cut) => cut.style)).toEqual([{ left: '25%', width: '35%' }])
    // Outside every clip there is nothing to mark.
    act(() => useStore.getState().setSelectedSecond(40))
    expect(result.current.toolbar.deletedPart.canStart).toBe(false)
  })

  test('Alt+drag on a clip deletes the dragged range instead of moving the clip', () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    const altPointer = (clientX) => ({
      ...createPointerEvent(clientX),
      altKey: true,
      currentTarget: {
        focus: vi.fn(),
        getBoundingClientRect: () => ({ left: 100, width: 200 }),
        releasePointerCapture: vi.fn(),
        setPointerCapture: vi.fn(),
      },
    })
    act(() => findLane(result, 'video:video-1').clipProps.onPointerDown(altPointer(250)))
    act(() => findLane(result, 'video:video-1').clipProps.onPointerMove(altPointer(150)))
    expect(findLane(result, 'video:video-1').cuts).toHaveLength(1)
    act(() => findLane(result, 'video:video-1').clipProps.onPointerUp(altPointer(150)))

    expect(useStore.getState().videoEdits['video-1']).toMatchObject({
      offsetSeconds: 10,
      deletedParts: [expect.objectContaining({ fromSecond: 5, toSecond: 15 })],
    })
    expect(useStore.getState().videoDragPreview).toBeNull()
  })
})
