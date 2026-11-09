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
})
