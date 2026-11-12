import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useActivityTrim from '@/features/player/hooks/useActivityTrim'
import useStore from '@/store/useStore'

function pointer(clientX) {
  return { button: 0, clientX, pointerId: 1, stopPropagation: vi.fn(), currentTarget: { setPointerCapture: vi.fn() } }
}

function renderTrim() {
  // 100 seconds over 500 pixels: 5 pixels per second.
  return renderHook(() => useActivityTrim({ durationSeconds: 100, viewStart: 0, viewEnd: 100, widthPx: 500 }))
}

describe('useActivityTrim', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('dragging an edge shows the draft and commits the trim once, on release', () => {
    const { result } = renderTrim()

    act(() => result.current.getEdgeDragProps('start').onPointerDown(pointer(0)))
    act(() => result.current.getEdgeDragProps('start').onPointerMove(pointer(50)))
    expect(result.current.trim).toEqual({ startSecond: 10, endSecond: 100 })
    expect(result.current.draggedEdge).toBe('start')
    expect(useStore.getState().activityTrim).toBeNull()

    act(() => result.current.getEdgeDragProps('start').onPointerUp(pointer(50)))
    expect(useStore.getState().activityTrim).toEqual({ startSecond: 10, endSecond: 100 })
    expect(result.current.draggedEdge).toBeNull()
  })

  test('a cancelled drag leaves the trim unchanged', () => {
    const { result } = renderTrim()

    act(() => result.current.getEdgeDragProps('end').onPointerDown(pointer(500)))
    act(() => result.current.getEdgeDragProps('end').onPointerMove(pointer(400)))
    act(() => result.current.getEdgeDragProps('end').onPointerCancel())

    expect(result.current.trim).toBeNull()
    expect(useStore.getState().activityTrim).toBeNull()
  })

  test('edges set at a second keep the other edge, and reset restores the whole activity', () => {
    const { result } = renderTrim()

    act(() => result.current.setEdge('end', 60))
    act(() => result.current.setEdge('start', 20))
    expect(useStore.getState().activityTrim).toEqual({ startSecond: 20, endSecond: 60 })
    expect(result.current.window).toEqual({ startSecond: 20, endSecond: 60, isTrimmed: true })

    act(() => result.current.reset())
    expect(useStore.getState().activityTrim).toBeNull()
  })
})
