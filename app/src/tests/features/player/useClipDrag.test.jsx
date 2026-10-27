import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vite-plus/test'
import useClipDrag from '@/features/player/hooks/useClipDrag'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

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

function createClips(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  return joinVideoEdits(importedVideos, videoEdits)
}

const SINGLE_CLIP = createClips([createTestVideo({ durationSeconds: 20 })], { 'video-1': 5 })

const TWO_CLIPS = createClips(
  [
    createTestVideo({ id: 'first', durationSeconds: 20 }),
    createTestVideo({ id: 'second', path: '/videos/second.mp4', durationSeconds: 10, importOrder: 1 }),
  ],
  { first: 5.03, second: 40 },
)

function renderDrag({ clips = SINGLE_CLIP } = {}) {
  const setVideoDragPreview = vi.fn()
  const setVideoOffset = vi.fn()
  const shiftVideoOffsets = vi.fn()
  const hook = renderHook(() => useClipDrag({ setVideoDragPreview, setVideoOffset, shiftVideoOffsets }))
  const containerElement = {
    getBoundingClientRect: () => ({ width: 250 }),
  }

  act(() => {
    hook.result.current.updateMetrics({
      activityDurationSeconds: 100,
      clips,
      containerElement,
      hasActivityData: true,
      timelineMinimum: 0,
      totalDuration: 100,
      viewEnd: 100,
      viewStart: 0,
      widthPx: 500,
    })
  })

  return { ...hook, setVideoDragPreview, setVideoOffset, shiftVideoOffsets }
}

function mockAnimationFrames() {
  const animationFrames = []
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    animationFrames.push(callback)
    return animationFrames.length
  })
  return animationFrames
}

describe('useClipDrag', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  test('keeps fractional movement live and rounds the committed offset on pointer up', () => {
    const animationFrames = mockAnimationFrames()
    const { result, setVideoDragPreview, setVideoOffset, shiftVideoOffsets } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerDown(createEvent(100, target))
    })

    expect(setVideoDragPreview).toHaveBeenLastCalledWith({ videoId: 'video-1', deltaSeconds: 0 })
    expect(result.current.isDragging).toBe(true)

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerMove(createEvent(103, target))
      animationFrames[0](16)
    })

    expect(setVideoDragPreview.mock.lastCall[0].videoId).toBe('video-1')
    expect(setVideoDragPreview.mock.lastCall[0].deltaSeconds).toBeCloseTo(1.2)

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerUp(createEvent(103, target))
    })

    expect(setVideoOffset).toHaveBeenLastCalledWith('video-1', 6.2)
    expect(shiftVideoOffsets).not.toHaveBeenCalled()
    expect(setVideoDragPreview).toHaveBeenLastCalledWith(null)
    expect(target.releasePointerCapture).toHaveBeenCalledWith(1)
    expect(result.current.isDragging).toBe(false)
  })

  test('snaps a clip edge to an alignment guideline within the pixel threshold', () => {
    const { result, setVideoDragPreview, setVideoOffset } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video:video-1').onPointerMove(createEvent(88, target))
    })

    expect(result.current.snapGuidelineSecond).toBe(0)

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerUp(createEvent(88, target))
    })

    expect(setVideoOffset).toHaveBeenLastCalledWith('video-1', 0)
    expect(setVideoDragPreview).toHaveBeenLastCalledWith(null)
    expect(result.current.snapGuidelineSecond).toBeNull()
  })

  test('snaps a dragged video clip to another clip edge', () => {
    const { result, setVideoOffset } = renderDrag({ clips: TWO_CLIPS })
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video:second').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video:second').onPointerMove(createEvent(63, target))
    })

    expect(result.current.snapGuidelineSecond).toBeCloseTo(25.03)

    act(() => {
      result.current.getLaneDragProps('video:second').onPointerUp(createEvent(63, target))
    })

    expect(setVideoOffset).toHaveBeenCalledTimes(1)
    expect(setVideoOffset.mock.calls[0][0]).toBe('second')
    expect(setVideoOffset.mock.calls[0][1]).toBe(25)
  })

  test('drags the activity lane by shifting every clip the opposite way', () => {
    const animationFrames = mockAnimationFrames()
    const { result, setVideoDragPreview, setVideoOffset, shiftVideoOffsets } = renderDrag({ clips: TWO_CLIPS })
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('activity').onPointerDown(createEvent(100, target))
    })

    expect(setVideoDragPreview).toHaveBeenLastCalledWith({ videoId: null, deltaSeconds: 0 })

    act(() => {
      result.current.getLaneDragProps('activity').onPointerMove(createEvent(103, target))
      animationFrames[0](16)
    })

    expect(setVideoDragPreview.mock.lastCall[0].videoId).toBeNull()
    expect(setVideoDragPreview.mock.lastCall[0].deltaSeconds).toBeCloseTo(-1.2)

    act(() => {
      result.current.getLaneDragProps('activity').onPointerUp(createEvent(103, target))
    })

    expect(setVideoOffset).not.toHaveBeenCalled()
    expect(shiftVideoOffsets).toHaveBeenCalledTimes(1)
    expect(shiftVideoOffsets.mock.calls[0][0]).toBeCloseTo(-1.23)
    expect(5.03 + shiftVideoOffsets.mock.calls[0][0]).toBeCloseTo(3.8)
    expect(setVideoDragPreview).toHaveBeenLastCalledWith(null)
  })

  test('snaps the activity lane so a clip edge lands on the activity start', () => {
    const { result, shiftVideoOffsets } = renderDrag({ clips: TWO_CLIPS })
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('activity').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('activity').onPointerMove(createEvent(112, target))
    })

    expect(result.current.snapGuidelineSecond).toBe(0)

    act(() => {
      result.current.getLaneDragProps('activity').onPointerUp(createEvent(112, target))
    })

    expect(shiftVideoOffsets).toHaveBeenCalledTimes(1)
    expect(shiftVideoOffsets.mock.calls[0][0]).toBeCloseTo(-5.03)
  })

  test('discards the transient shift when the pointer drag is cancelled', () => {
    const { result, setVideoDragPreview, setVideoOffset, shiftVideoOffsets } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.getLaneDragProps('video:video-1').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video:video-1').onPointerMove(createEvent(103, target))
      result.current.getLaneDragProps('video:video-1').onPointerCancel(createEvent(103, target))
    })

    expect(setVideoOffset).not.toHaveBeenCalled()
    expect(shiftVideoOffsets).not.toHaveBeenCalled()
    expect(setVideoDragPreview).toHaveBeenLastCalledWith(null)
    expect(result.current.isDragging).toBe(false)
  })

  test('commits a negative offset while the video still overlaps the activity', () => {
    const { result, setVideoOffset } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.updateMetrics({ timelineMinimum: -10, viewEnd: 90, viewStart: -10 })
      result.current.getLaneDragProps('video:video-1').onPointerDown(createEvent(100, target))
      result.current.getLaneDragProps('video:video-1').onPointerMove(createEvent(80, target))
      result.current.getLaneDragProps('video:video-1').onPointerUp(createEvent(80, target))
    })

    expect(setVideoOffset).toHaveBeenLastCalledWith('video-1', -3)
  })

  test('ignores lane drags without imported clips', () => {
    const { result, setVideoDragPreview } = renderDrag({ clips: [] })

    act(() => {
      result.current.getLaneDragProps('activity').onPointerDown(createEvent(100))
    })

    expect(setVideoDragPreview).not.toHaveBeenCalled()
    expect(result.current.isDragging).toBe(false)
  })

  test('accumulates keyboard nudges per lane and commits them as one edit', () => {
    const { result, setVideoDragPreview, setVideoOffset, shiftVideoOffsets } = renderDrag({ clips: TWO_CLIPS })

    act(() => {
      result.current.nudgeLane('video:first', -0.1)
      result.current.nudgeLane('video:first', -0.1)
    })

    expect(setVideoDragPreview.mock.lastCall[0].videoId).toBe('first')
    expect(setVideoDragPreview.mock.lastCall[0].deltaSeconds).toBeCloseTo(-0.2)

    act(() => {
      result.current.commitNudge()
    })

    expect(setVideoOffset).toHaveBeenLastCalledWith('first', 4.8)
    expect(setVideoDragPreview).toHaveBeenLastCalledWith(null)

    act(() => {
      result.current.nudgeLane('activity', 0.1)
    })

    expect(setVideoDragPreview).toHaveBeenLastCalledWith({ videoId: null, deltaSeconds: -0.1 })

    act(() => {
      result.current.commitNudge()
    })

    expect(shiftVideoOffsets).toHaveBeenCalledTimes(1)
    expect(5.03 + shiftVideoOffsets.mock.calls[0][0]).toBeCloseTo(4.9)
  })

  test('reduces autoscroll edge penetration when the timeline is zoomed in', () => {
    const animationFrames = mockAnimationFrames()
    const followSecond = vi.fn(() => null)
    const { result } = renderDrag()
    const target = createTarget()

    act(() => {
      result.current.updateMetrics({ followSecond, viewStart: 40, viewEnd: 60 })
      result.current.getLaneDragProps('video:video-1').onPointerDown(createEvent(125, target))
      result.current.getLaneDragProps('video:video-1').onPointerMove(createEvent(25, target))
      animationFrames[1](16)
    })

    const targetSecond = followSecond.mock.calls[0][0]
    expect(targetSecond).toBeGreaterThan(42)
    expect(targetSecond).toBeLessThan(43)
    expect(followSecond).toHaveBeenCalledWith(targetSecond, -3)
  })
})
