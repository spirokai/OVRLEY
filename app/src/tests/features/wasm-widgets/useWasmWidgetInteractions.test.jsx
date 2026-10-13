import { act, renderHook } from '@testing-library/react'
import { expect, test, vi } from 'vitest'
import { useWasmWidgetInteractions } from '@/features/wasm-widgets/hooks/useWasmWidgetInteractions'
import { registerWidgetInteraction, unregisterWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'

test.each(['speed', 'gradient', 'backdrop', 'raster'])('%s resize commits once on release; Escape cancels without committing', (type) => {
  const widget = { id: type, type, data: { x: 100, y: 50, rotation: 0 } }
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
    result.current.onResize({ width: 240, height: 75.5 })
  })
  expect(controller.update).toHaveBeenCalledWith({
    type: 'resize',
    geometry: { ...(type === 'backdrop' || type === 'raster' ? { size: [240, 75.5] } : { factor: 1.5 }), fixed: [1, 1], point: [240, 95] },
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
