import { act, renderHook } from '@testing-library/react'
import { expect, test, vi } from 'vitest'
import { useWasmWidgetInteractions } from '@/features/wasm-widgets/hooks/useWasmWidgetInteractions'
import { registerWidgetInteraction, unregisterWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'

test('resize commits once on release; Escape cancels the controller and prevents a later end event from committing', () => {
  const widget = { id: 'speed', data: { x: 100, y: 50 } }
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
    beginWidgetInteraction: vi.fn(),
    clearWidgetDrafts: vi.fn(),
    endWidgetInteraction: vi.fn(),
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
    result.current.onResize({ width: 240 })
  })
  expect(controller.update).toHaveBeenCalledWith({ type: 'resize', geometry: { factor: 1.5, fixed: [1, 1], point: [240, 95] } })
  expect(state.commitWidgetUpdate).not.toHaveBeenCalled()
  act(() => result.current.onResizeEnd())
  expect(state.commitWidgetUpdate).toHaveBeenCalledExactlyOnceWith('speed', patch)
  act(() => {
    result.current.onResizeStart(start)
    result.current.onResize({ width: 320 })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    result.current.onResizeEnd()
  })
  expect(controller.finish).toHaveBeenCalledTimes(1)
  expect(controller.cancel).toHaveBeenCalled()
  expect(state.moveableRef.current.stopDrag).toHaveBeenCalledTimes(1)
  expect(state.commitWidgetUpdate).toHaveBeenCalledTimes(1)
  expect(state.clearWidgetDrafts).toHaveBeenLastCalledWith(['speed'])
  unmount()
  unregisterWidgetInteraction(target)
})
