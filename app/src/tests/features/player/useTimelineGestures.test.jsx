import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import useTimelineGestures from '@/features/player/hooks/useTimelineGestures'

function createTarget() {
  return {
    getBoundingClientRect: () => ({ left: 0, width: 100 }),
    hasPointerCapture: vi.fn(() => true),
    releasePointerCapture: vi.fn(),
    setPointerCapture: vi.fn(),
  }
}

function createEvent(overrides = {}) {
  return {
    button: 0,
    clientX: 0,
    currentTarget: createTarget(),
    pointerId: 1,
    stopPropagation: vi.fn(),
    ...overrides,
  }
}

function renderGestures() {
  const commands = {
    cancelMarkerPreview: vi.fn(),
    cancelScrub: vi.fn(),
    commitMarker: vi.fn(),
    commitScrub: vi.fn(),
    previewMarker: vi.fn(),
    scrubTo: vi.fn(),
  }
  const result = renderHook((props) => useTimelineGestures(props), {
    initialProps: commands,
  })

  act(() => {
    result.result.current.updateTimelineMetrics({
      containerElement: null,
      panBy: commands.panBy,
      totalDuration: 100,
      viewEnd: 100,
      viewStart: 0,
      widthPx: 100,
    })
  })

  return { commands, ...result }
}

describe('useTimelineGestures', () => {
  test('scrubs and commits direct second values from the axis', () => {
    const { commands, result } = renderGestures()

    act(() => {
      result.current.axisProps.onPointerDown(createEvent({ clientX: 25 }))
    })

    expect(commands.scrubTo).toHaveBeenCalledWith(25)
    expect(result.current.isTimelineDragging).toBe(true)

    act(() => {
      result.current.axisProps.onPointerUp(createEvent({ clientX: 30 }))
    })

    expect(commands.commitScrub).toHaveBeenCalledWith(30)
    expect(result.current.isTimelineDragging).toBe(false)

    act(() => {
      result.current.axisProps.onPointerDown(createEvent({ clientX: 35 }))
      result.current.axisProps.onPointerCancel(createEvent({ clientX: 35 }))
    })

    expect(commands.cancelScrub).toHaveBeenCalledTimes(1)
    expect(result.current.isTimelineDragging).toBe(false)
  })

  test('pans by pixel delta converted to seconds', () => {
    const { result } = renderGestures()
    const panBy = vi.fn()

    act(() => {
      result.current.updateTimelineMetrics({
        panBy,
        totalDuration: 100,
        viewEnd: 100,
        viewStart: 0,
        widthPx: 100,
      })
      result.current.panSurfaceProps.onPointerDown(createEvent({ clientX: 70 }))
      result.current.panSurfaceProps.onPointerMove(createEvent({ clientX: 60 }))
    })

    expect(panBy).toHaveBeenCalledWith(10)
  })

  test('previews, commits, and cancels export marker drags', () => {
    const { commands, result } = renderGestures()
    const markerProps = result.current.getExportMarkerProps('from')

    act(() => {
      markerProps.onPointerDown(createEvent({ clientX: 10 }))
      markerProps.onPointerMove(createEvent({ clientX: 15 }))
      markerProps.onPointerUp(createEvent({ clientX: 20 }))
    })

    expect(commands.previewMarker).toHaveBeenNthCalledWith(1, 'from', 10)
    expect(commands.previewMarker).toHaveBeenNthCalledWith(2, 'from', 15)
    expect(commands.commitMarker).toHaveBeenCalledWith('from', 20)

    act(() => {
      markerProps.onPointerDown(createEvent({ clientX: 10 }))
      markerProps.onPointerCancel(createEvent({ clientX: 10 }))
    })

    expect(commands.cancelMarkerPreview).toHaveBeenCalled()
  })
})
