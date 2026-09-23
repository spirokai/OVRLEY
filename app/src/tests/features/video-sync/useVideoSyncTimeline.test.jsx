import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useVideoSyncTimeline from '@/features/video-sync/hooks/useVideoSyncTimeline'

const container = {
  getBoundingClientRect: () => ({ left: 100, right: 500, width: 400 }),
}

const baseProps = {
  containerElement: container,
  detection: null,
  enabled: true,
  landmarks: [{ id: 'mark-1', type: 'stop', videoSecond: 3 }],
  moveLandmark: vi.fn(),
  scrubTo: vi.fn(),
  timelineMinimum: 0,
  totalDuration: 20,
  videoDuration: 20,
  videoSyncOffsetSeconds: 2,
  videoSyncOffsetPreviewSeconds: null,
  viewEnd: 20,
  viewStart: 0,
  widthPx: 400,
}

function pointerEvent(clientX, currentTarget = {}) {
  return {
    button: 0,
    clientX,
    currentTarget: {
      hasPointerCapture: () => true,
      releasePointerCapture: vi.fn(),
      setPointerCapture: vi.fn(),
      ...currentTarget,
    },
    pointerId: 1,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  }
}

describe('useVideoSyncTimeline', () => {
  test('builds populated graph paths on the initial enabled render', () => {
    const detection = {
      graphSeries: {
        speed: [
          { time: 0, value: 1 },
          { time: 10, value: 2 },
        ],
        turning: [
          { time: 0, value: 0 },
          { time: 10, value: 30 },
        ],
      },
      location: null,
      stops: [],
      turns: [],
    }
    const { result } = renderHook(() => useVideoSyncTimeline({ ...baseProps, detection }))

    expect(result.current.graph.paths.speed).not.toBe('')
    expect(result.current.graph.paths.turning).not.toBe('')
  })

  test('commits one video-local landmark move and never changes the video offset', () => {
    const moveLandmark = vi.fn()
    const { result } = renderHook((props) => useVideoSyncTimeline(props), { initialProps: { ...baseProps, moveLandmark } })
    const initialTimelineSecond = result.current.landmarks[0].timelineSecond

    act(() => result.current.landmarks[0].handleProps.onPointerDown(pointerEvent(220)))
    act(() => result.current.landmarks[0].handleProps.onPointerMove(pointerEvent(300)))
    act(() => result.current.landmarks[0].handleProps.onPointerUp(pointerEvent(300)))

    expect(moveLandmark).toHaveBeenCalledTimes(1)
    expect(moveLandmark).toHaveBeenCalledWith('mark-1', 8)
    expect(initialTimelineSecond).toBe(5)
  })

  test('uses the preview offset to move landmark display geometry without committing a landmark', () => {
    const moveLandmark = vi.fn()
    const { result, rerender } = renderHook((props) => useVideoSyncTimeline(props), { initialProps: { ...baseProps, moveLandmark } })

    rerender({ ...baseProps, moveLandmark, videoSyncOffsetPreviewSeconds: 5 })

    expect(result.current.landmarks[0].timelineSecond).toBe(8)
    expect(moveLandmark).not.toHaveBeenCalled()
  })

  test('clamps idle offscreen landmarks to an inert viewport-edge indicator', () => {
    const { result } = renderHook((props) => useVideoSyncTimeline(props), {
      initialProps: { ...baseProps, landmarks: [{ id: 'late', type: 'rightTurn', videoSecond: 15 }], viewEnd: 10 },
    })

    expect(result.current.landmarks[0]).toMatchObject({ handleProps: null, isClamped: true, isInteractive: false })
    expect(result.current.landmarks[0].handleStyle.left).toBe(400)
  })
})
