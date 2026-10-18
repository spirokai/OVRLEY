import { act, renderHook } from '@testing-library/react'
import { expect, test, vi } from 'vite-plus/test'
import { useWasmWidgetInteractions } from '@/features/wasm-widgets/hooks/useWasmWidgetInteractions'
import { createWidgetInteraction, registerWidgetInteraction, unregisterWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import useWidgetDraftState, { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'

test.each([
  ['speed', 'text'],
  ['gradient', 'text'],
  ['lean_angle', 'lean_angle'],
  ['backdrop', 'rectangle'],
  ['raster', 'raster'],
  ['course', undefined],
  ['elevation', undefined],
  ['lap_timer', 'lap_timer'],
  ['heading', 'heading_tape'],
  ['power', 'linear'],
  ['speed', 'arc'],
  ['altitude', 'corner'],
])('%s/%s resize commits once; Escape cancels', (type, displayType) => {
  const widget = { id: type, type, data: { x: 100, y: 50, rotation: 0, display_type: displayType } }
  const target = document.createElement('div')
  const patch = { x: 70.25, y: 35.5, font_size: 72 }
  const controller = {
    begin: vi.fn(() => ({ data: widget.data, bounds: { left: 80, top: 55, width: 160, height: 40 } })),
    update: vi.fn(),
    finish: vi.fn(() => patch),
    cancel: vi.fn(),
  }
  registerWidgetInteraction(target, controller)
  const state = {
    config: {},
    sceneElement: document.createElement('div'),
    setLiveWidgetDraft: vi.fn(),
    clearWidgetDrafts: vi.fn(),
    commitWidgetUpdate: vi.fn(),
    moveableRef: { current: { stopDrag: vi.fn(), updateRect: vi.fn() } },
  }
  const { result, unmount } = renderHook(() =>
    useWasmWidgetInteractions({
      selectedWidget: widget,
      selectedWidgets: [widget],
      state,
      setIsGroupDragActive: vi.fn(),
      setGroupDragSelectionIds: vi.fn(),
    }),
  )
  const start = { target, direction: [-1, -1], set: vi.fn(), dragStart: false }
  act(() => {
    result.current.onResizeStart(start)
    result.current.onResize({ width: 240, height: 75.5 })
  })
  expect(controller.update).toHaveBeenCalledWith({
    type: 'resize',
    geometry: {
      ...(type === 'backdrop' || type === 'raster' || type === 'elevation' || ['heading_tape', 'linear'].includes(displayType)
        ? { size: [240, 75.5] }
        : { factor: 1.5 }),
      fixed: [1, 1],
      point: [240, 95],
    },
  })
  expect(state.commitWidgetUpdate).not.toHaveBeenCalled()
  act(() => result.current.onResizeEnd())
  expect(state.commitWidgetUpdate).toHaveBeenCalledExactlyOnceWith(type, patch)
  act(() => {
    result.current.onResizeStart(start)
    result.current.onResize({ width: 320, height: 80 })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    result.current.onResizeEnd()
  })
  expect(controller.finish).toHaveBeenCalledTimes(1)
  expect(controller.cancel).toHaveBeenCalled()
  expect(state.moveableRef.current.stopDrag).toHaveBeenCalledTimes(1)
  expect(state.commitWidgetUpdate).toHaveBeenCalledTimes(1)
  expect(state.clearWidgetDrafts).toHaveBeenLastCalledWith([type])
  unmount()
  unregisterWidgetInteraction(target)
})

test('group translation shares canonical data patches and commits once before a pending frame', () => {
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
  try {
    const selectedWidgets = [
      { id: 'speed', type: 'speed', data: { x: 100, y: 50 } },
      { id: 'label', type: 'label', data: { x: 200, y: 75 } },
    ]
    const commitWidgetUpdates = vi.fn()
    const config = {}
    const sceneElement = document.createElement('div')
    const moveableRef = { current: { stopDrag: vi.fn(), updateRect: vi.fn() } }
    const target = document.createElement('div')
    let frame = null
    const transient = vi.fn()
    const render = () => {
      frame = null
      interaction.resolve({ widgets: selectedWidgets, scene: {}, second: 0 })
      transient(selectedWidgets.map(({ id }) => interaction.isDragging(id)))
    }
    const interaction = createWidgetInteraction({
      schedule() {
        if (frame === null) frame = requestAnimationFrame(render)
      },
      flush() {
        if (frame !== null) cancelAnimationFrame(frame)
        render()
      },
      getSnapshot: () => ({ widgets: selectedWidgets, scene: {}, second: 0 }),
      getGeometry: () => ({ content: { left: 100, top: 50, width: 50, height: 20 } }),
      resize: vi.fn(),
    })
    registerWidgetInteraction(target, interaction.forWidget('speed'))
    const { result, unmount } = renderHook(() => {
      const controller = useWidgetDraftState()
      const liveEdits = useWidgetDraftView(controller)
      const state = { ...liveEdits, config, sceneElement, moveableRef, commitWidgetUpdates }
      const handlers = useWasmWidgetInteractions({
        selectedWidget: selectedWidgets[0],
        selectedWidgets,
        state,
        setIsGroupDragActive: vi.fn(),
        setGroupDragSelectionIds: vi.fn(),
      })
      return { controller, liveEdits, handlers }
    })
    const events = [
      { target, set: vi.fn(), beforeTranslate: [12.125, 8.25] },
      { set: vi.fn(), beforeTranslate: [12.125, 8.25] },
    ]
    act(() => {
      result.current.handlers.onDragGroupStart({ events })
      result.current.handlers.onDragGroup({ events, inputEvent: { ctrlKey: false } })
    })
    act(() => vi.advanceTimersToNextFrame())
    expect(transient).toHaveBeenLastCalledWith([true, true])
    expect(result.current.controller.getWidgetDraft('speed')).toEqual({ x: 112.125, y: 58.25 })
    expect(result.current.controller.getWidgetDraft('label')).toEqual({ x: 212.125, y: 83.25 })
    expect(commitWidgetUpdates).not.toHaveBeenCalled()
    act(() => {
      result.current.handlers.onDragGroup({ events: [{ beforeTranslate: [20.25, 10.5] }], inputEvent: { ctrlKey: false } })
      result.current.handlers.onDragGroupEnd()
    })
    expect(transient).toHaveBeenLastCalledWith([false, false])
    expect(commitWidgetUpdates).toHaveBeenCalledExactlyOnceWith({
      speed: { x: 120.25, y: 60.5 },
      label: { x: 220.25, y: 85.5 },
    })
    expect(result.current.liveEdits.liveWidgetDrafts).toEqual({})
    expect(vi.getTimerCount()).toBe(0)
    unmount()
    unregisterWidgetInteraction(target)
  } finally {
    vi.useRealTimers()
  }
})
