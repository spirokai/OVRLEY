import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useOverlayPlayer from '@/features/player/hooks/useOverlayPlayer'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function createTimelineElement(width = 500) {
  return {
    offsetWidth: width,
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
    // The SVG origin sits half the head width left of the line center at 250px, snapped to a whole screen pixel.
    expect(result.current.timeline.playhead.style.transform).toBe('translateX(244px)')
  })

  test('hides the complete export-range presentation in video-sync mode', () => {
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black', videoSyncMode: true }))

    act(() => {
      result.current.timeline.containerProps.ref(createTimelineElement(500))
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
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

  test('cuts and excludes at the playhead with C and X', () => {
    useStore.setState({ selectedSecond: 15 })
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    const press = (key) => act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key })))
    const edits = () => useStore.getState().videoEdits['video-1']
    const markers = () => result.current.timeline.lanes.find((lane) => lane.id === 'video').cutMarkers
    expect(result.current.toolbar.videoEdit).toMatchObject({ isVisible: true, canCut: true, excluded: false })

    press('c')
    const [cut] = edits().cuts
    expect(cut.frame).toBe(150)
    expect(markers()).toEqual([expect.objectContaining({ label: 'Cut at 00:05', style: { left: expect.any(String) } })])
    // The playhead is on the cut now.
    expect(result.current.toolbar.videoEdit.canCut).toBe(false)

    press('x')
    expect(edits().segments).toEqual({ start: { excluded: false, transitionIn: null }, [cut.id]: { excluded: true, transitionIn: null } })
    expect(result.current.toolbar.videoEdit.excluded).toBe(true)
    expect(findLane(result, 'video:video-1').excludedSegments.map((segment) => segment.style)).toEqual([{ left: '25%', width: '75%' }])

    act(() => markers()[0].onRemove())
    expect(useStore.getState().cutRemovalRequest).toEqual({ videoId: 'video-1', cutId: cut.id })

    // Outside every clip there is nothing to cut or exclude.
    act(() => useStore.getState().setSelectedSecond(40))
    expect(result.current.toolbar.videoEdit).toMatchObject({ canCut: false, excluded: null })
    press('c')
    expect(edits().cuts).toHaveLength(1)
  })

  test('a clip keeps its selection while the cut button is pressed', () => {
    createTwoClipStore()
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    act(() =>
      findLane(result, 'video:second').clipProps.onPointerDown({
        ...createPointerEvent(100),
        currentTarget: { focus: vi.fn(), setPointerCapture: vi.fn() },
      }),
    )
    expect(findLane(result, 'video:second').isSelected).toBe(true)

    const button = document.createElement('button')
    const controls = document.createElement('div')
    controls.setAttribute('data-keeps-clip-selection', '')
    controls.append(button)
    document.body.append(controls)
    act(() => button.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })))
    expect(findLane(result, 'video:second').isSelected).toBe(true)
    act(() => document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })))
    expect(findLane(result, 'video:second').isSelected).toBe(false)
    controls.remove()
  })

  test('marks each join on the video lane with its transition window and control', () => {
    createTwoClipStore()
    useStore.setState({ transitionDefaults: { type: 'wipeleft', durationSeconds: 2, splitGapThresholdSeconds: 2 } })
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    const markers = () => result.current.timeline.lanes.find((lane) => lane.id === 'video').joinMarkers

    // A 20 s gap is above the continuity threshold: the default transition applies.
    expect(markers()).toEqual([expect.objectContaining({ key: 'second:start', hasTransition: true, isInvalid: false, label: 'Join: wipeleft, 2 s' })])
    act(() => markers()[0].control.onSelect('none'))
    expect(useStore.getState().videoEdits.second.segments.start.transitionIn).toEqual({ type: 'none', durationSeconds: 0 })
    expect(markers()[0]).toMatchObject({ hasTransition: false, bandStyle: null, label: 'Join: No transition' })
  })
})
