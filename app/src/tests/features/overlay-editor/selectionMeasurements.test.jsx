import { fireEvent, renderHook } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vite-plus/test'
import { useSelectionBorder } from '@/features/overlay-editor/hooks/useSelectionBorder'
import { buildWidgetBadges } from '@/features/overlay-editor/utils/selectionGeometry'

afterEach(() => vi.unstubAllGlobals())

test('badges use fractional content geometry and its rotation pivot without reading DOM rectangles', () => {
  const sceneElement = document.createElement('div')
  const node = document.createElement('div')
  sceneElement.getBoundingClientRect = node.getBoundingClientRect = () => {
    throw new Error('Badge placement must not measure layout')
  }
  const [badge] = buildWidgetBadges({
    displayScale: 0.5,
    getGeometry: () => ({ content: { left: 100.25, top: 80.5, width: 100, height: 40 }, rotation: 90, rotation_origin: [50, 20] }),
    hoveredWidgetId: null,
    orderedWidgetIds: ['route'],
    sceneElement,
    selectedWidgetIds: ['route'],
    widgetMap: { route: { type: 'course' } },
    widgetNodes: { route: node },
  })
  expect(badge.left).toBeCloseTo(105.125)
  expect(badge.top).toBeCloseTo(27.25)
  expect(badge.rotation).toBe(90)
})

test('border movement reuses viewport measurements until scrolling', () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
  vi.stubGlobal('devicePixelRatio', 2)
  const viewport = document.createElement('div')
  viewport.dataset.overlayViewport = ''
  const container = viewport.appendChild(document.createElement('div'))
  Object.defineProperties(container, { offsetWidth: { value: 200 }, offsetHeight: { value: 100 } })
  const bounds = { left: 10, top: 20, right: 110, bottom: 70, width: 100, height: 50 }
  const measure = vi.fn(() => bounds)
  container.getBoundingClientRect = viewport.getBoundingClientRect = measure
  const { result, rerender } = renderHook((vertices) => useSelectionBorder(container, vertices, 1), {
    initialProps: [[{ x: 0, y: 0 }]],
  })
  rerender([[{ x: 20, y: 10 }]])
  expect(result.current.polygons).toEqual(['20,25'])
  expect(measure).toHaveBeenCalledTimes(2)

  measure.mockReturnValue({ ...bounds, top: 15 })
  fireEvent.scroll(window)
  expect(result.current.polygons).toEqual(['20,20'])
  expect(measure).toHaveBeenCalledTimes(4)
})
