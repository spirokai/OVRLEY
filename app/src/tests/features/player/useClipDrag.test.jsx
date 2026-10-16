import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vite-plus/test'
import useClipDrag from '@/features/player/hooks/useClipDrag'

function createTarget() {
  return {
    releasePointerCapture: vi.fn(),
    setPointerCapture: vi.fn(),
  }
}

function createEvent(clientX, currentTarget = createTarget()) {
  return {
    button: 0,
    clientX,
    currentTarget,
    pointerId: 1,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  }
}

function renderDrag(setVideoSyncOffset = vi.fn(), setVideoSyncOffsetPreview = vi.fn()) {
  const hook = renderHook(() => useClipDrag({ setVideoSyncOffset, setVideoSyncOffsetPreview }))
  const containerElement = {
    getBoundingClientRect: () => ({ width: 250 }),
  }

  act(() => {
    hook.result.current.updateMetrics({
      activityDurationSeconds: 100,
      containerElement,
      hasVideo: true,
      importedVideoDuration: 20,
      timelineMinimum: 0,
      totalDuration: 100,
      videoSyncOffsetSeconds: 5,
      viewEnd: 100,
      viewStart: 0,
      widthPx: 500,
    })
  })

  return { ...hook, setVideoSyncOffset, setVideoSyncOffsetPreview }
}

describe('useClipDrag', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  test('keeps fractional movement live and rounds the committed offset on pointer up', () => {
    const setVideoSyncOffset = vi.fn()
    const setVideoSyncOffsetPreview = vi.fn()
    const { result } = renderDrag(setVideoSyncOffset, setVideoSyncOffsetPreview)
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video').onPointerDown(createEvent(100, target))
    })

    expect(setVideoSyncOffsetPreview).toHaveBeenLastCalledWith(5)

    act(() => {
      result.current.getLaneDragProps('video').onPointerMove(createEvent(103, target))
    })

    act(() => {
      result.current.getLaneDragProps('video').onPointerUp(createEvent(103, target))
    })

    expect(setVideoSyncOffset).toHaveBeenLastCalledWith(6.2)
    expect(setVideoSyncOffsetPreview).toHaveBeenLastCalledWith(null)
    expect(target.releasePointerCapture).toHaveBeenCalledWith(1)
  })

  test('snaps a clip edge to an alignment guideline within the pixel threshold', () => {
    const setVideoSyncOffset = vi.fn()
    const setVideoSyncOffsetPreview = vi.fn()
    const { result } = renderDrag(setVideoSyncOffset, setVideoSyncOffsetPreview)
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video').onPointerMove(createEvent(88, target))
    })

    act(() => {
      result.current.getLaneDragProps('video').onPointerUp(createEvent(88, target))
    })

    expect(setVideoSyncOffset).toHaveBeenLastCalledWith(0)
    expect(setVideoSyncOffsetPreview).toHaveBeenLastCalledWith(null)
  })

  test('restores the initial offset when the pointer drag is cancelled', () => {
    const setVideoSyncOffset = vi.fn()
    const setVideoSyncOffsetPreview = vi.fn()
    const { result } = renderDrag(setVideoSyncOffset, setVideoSyncOffsetPreview)
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video').onPointerMove(createEvent(103, target))
      result.current.getLaneDragProps('video').onPointerCancel(createEvent(103, target))
    })

    expect(setVideoSyncOffset).toHaveBeenLastCalledWith(5)
    expect(setVideoSyncOffsetPreview).toHaveBeenLastCalledWith(null)
  })

  test('commits a negative offset while the video still overlaps the activity', () => {
    const setVideoSyncOffset = vi.fn()
    const { result } = renderDrag(setVideoSyncOffset)
    const target = createTarget()

    act(() => {
      result.current.updateMetrics({ timelineMinimum: -10, viewEnd: 90, viewStart: -10 })
      result.current.getLaneDragProps('video').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video').onPointerMove(createEvent(80, target))
      result.current.getLaneDragProps('video').onPointerUp(createEvent(80, target))
    })

    expect(setVideoSyncOffset).toHaveBeenLastCalledWith(-3)
  })

  test('reduces autoscroll edge penetration when the timeline is zoomed in', () => {
    const animationFrames = []
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      animationFrames.push(callback)
      return animationFrames.length
    })
    const followSecond = vi.fn(() => null)
    const { result } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.updateMetrics({ followSecond, viewStart: 40, viewEnd: 60 })
      result.current.getLaneDragProps('video').onPointerDown(createEvent(125, target))
      result.current.getLaneDragProps('video').onPointerMove(createEvent(25, target))
      animationFrames[1](16)
    })

    const targetSecond = followSecond.mock.calls[0][0]
    expect(targetSecond).toBeGreaterThan(42)
    expect(targetSecond).toBeLessThan(43)
    expect(followSecond).toHaveBeenCalledWith(targetSecond, -3)
  })
})
