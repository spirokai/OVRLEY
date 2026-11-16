import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vite-plus/test'
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

function timelineClips(result) {
  return result.current.timeline.lanes.flatMap((lane) => lane.clips)
}

function findLane(result, clipId) {
  return timelineClips(result).find((clip) => clip.id === clipId)
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

  test('keeps the clip selection for clicks in kept areas and the popups and dialogs they open, and ends it on other clicks', () => {
    renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    const selection = { laneId: 'video:video-1', segmentKey: null }
    act(() => useStore.getState().selectClip(selection))
    const pointerDownOn = (attributes) => {
      const element = document.createElement('div')
      for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, value)
      document.body.append(element)
      act(() => element.dispatchEvent(new Event('pointerdown', { bubbles: true })))
      element.remove()
    }

    pointerDownOn({ 'data-keeps-clip-selection': '' })
    pointerDownOn({ 'data-radix-popper-content-wrapper': '' })
    pointerDownOn({ role: 'dialog' })
    expect(useStore.getState().selectedClip).toEqual(selection)

    pointerDownOn({})
    expect(useStore.getState().selectedClip).toBeNull()
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

  test('trims the video at the playhead with [ ] and I/O when videos are loaded', () => {
    useStore.setState({ selectedSecond: 15 })
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    const edits = () => useStore.getState().videoEdits['video-1']
    expect(result.current.toolbar.trim).toMatchObject({ isVisible: true, target: 'video', canTrimStart: true, canTrimEnd: true })

    act(() => result.current.toolbar.trim.trimStart())
    expect(edits().cuts.map((cut) => cut.frame)).toEqual([150])
    expect(edits().segments.start.excluded).toBe(true)

    // A second trim of the same end moves its trim cut.
    act(() => useStore.setState({ selectedSecond: 12 }))
    act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' })))
    expect(edits().cuts.map((cut) => cut.frame)).toEqual([60])
    expect(useStore.getState().activityTrim).toBeNull()

    act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'o' })))
    expect(edits().cuts.map((cut) => cut.frame)).toEqual([60])
    act(() => useStore.setState({ selectedSecond: 25 }))
    act(() => result.current.toolbar.trim.trimEnd())
    expect(edits().cuts.map((cut) => cut.frame)).toEqual([60, 450])
    expect(edits().segments[edits().cuts[1].id].excluded).toBe(true)
  })

  test('routes playback, activity trim, mute, and synchronization shortcuts', () => {
    resetStore({ videos: [], offsets: {} })
    const { result, unmount } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    expect(result.current.toolbar.trim).toMatchObject({ isVisible: true, target: 'activity', canTrimStart: true, canTrimEnd: true })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
    })
    expect(useStore.getState().selectedSecond).toBe(1)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    })
    expect(useStore.getState().activityTrim).toEqual({ startSecond: 1, endSecond: 100 })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home' }))
    })
    expect(useStore.getState().selectedSecond).toBe(0)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i', altKey: true }))
    })
    expect(useStore.getState().selectedSecond).toBe(1)

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', ctrlKey: true }))
    })
    expect(useStore.getState().activityTrim).toBeNull()

    // Mute and synchronization need a video.
    act(() => useStore.setState(videoStoreState([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })], { 'video-1': 10 })))

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

  test('shows the activity trim with handles on the activity lane, and leaves it alone in video-sync mode', () => {
    useStore.setState({ activityTrim: { startSecond: 20, endSecond: 60 } })
    const activityLane = (result) => result.current.timeline.lanes.find((lane) => lane.id === 'activity')
    const { result, unmount } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    act(() => result.current.timeline.containerProps.ref(createTimelineElement(500)))
    expect(activityLane(result).clips[0].trimMasks.map((mask) => mask.key)).toEqual(['start', 'end'])
    expect(activityLane(result).cutMarkers.map((marker) => marker.key)).toEqual(['activity:start', 'activity:end'])
    unmount()

    const sync = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black', videoSyncMode: true }))
    act(() => {
      sync.result.current.timeline.containerProps.ref(createTimelineElement(500))
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', ctrlKey: true }))
    })
    expect(activityLane(sync.result).clips[0].trimMasks).toEqual([])
    expect(activityLane(sync.result).cutMarkers).toEqual([])
    expect(useStore.getState().activityTrim).toEqual({ startSecond: 20, endSecond: 60 })
  })

  test('cuts and excludes at the playhead with C and X', () => {
    useStore.setState({ selectedSecond: 15 })
    const { result } = renderHook(() => useOverlayPlayer({ activeKeyboardWorkspace: 'player', backgroundMode: 'black' }))
    // Cut markers show only on a clip wide enough on screen to drag them apart from the clip.
    act(() => result.current.timeline.containerProps.ref(createTimelineElement(500)))
    const press = (key) => act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key })))
    const edits = () => useStore.getState().videoEdits['video-1']
    // Real cuts only: the clip's edges carry trim handles too.
    const markers = () => result.current.timeline.lanes.find((lane) => lane.id === 'video').cutMarkers.filter((marker) => marker.onRemove !== null)
    expect(result.current.toolbar.videoEdit).toMatchObject({ isVisible: true, canCut: true, excluded: false })

    press('c')
    const [cut] = edits().cuts
    expect(cut.frame).toBe(150)
    expect(markers()).toEqual([expect.objectContaining({ label: 'Cut at 00:05', style: { left: expect.any(String) } })])
    // The playhead is on the cut now.
    expect(result.current.toolbar.videoEdit.canCut).toBe(false)

    press('x')
    expect(edits().segments).toEqual({
      start: { excluded: false, transitionIn: null, skipOverlay: false },
      [cut.id]: { excluded: true, transitionIn: null, skipOverlay: false },
    })
    expect(result.current.toolbar.videoEdit.excluded).toBe(true)
    expect(
      findLane(result, 'video:video-1')
        .segments.filter((segment) => segment.excluded)
        .map((segment) => segment.style),
    ).toEqual([{ left: '25%', width: '75%' }])

    act(() => markers()[0].onRemove())
    expect(useStore.getState().cutRemovalRequest).toEqual({ videoId: 'video-1', cutId: cut.id })

    // Outside every clip there is nothing to cut or exclude.
    act(() => useStore.getState().setSelectedSecond(40))
    expect(result.current.toolbar.videoEdit).toMatchObject({ canCut: false, excluded: null })
    press('c')
    expect(edits().cuts).toHaveLength(1)
  })
})
