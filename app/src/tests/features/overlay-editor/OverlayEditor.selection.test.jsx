/**
 * Integration tests for overlay-editor selection behavior.
 *
 * The editor should drive selection through pointer intents while the shared
 * store remains the single owner of the selected-id list and primary widget.
 */

import { fireEvent, render } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import OverlayEditor from '@/features/overlay-editor/components/OverlayEditor'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path) => path,
}))

vi.mock('@/features/video-preview', () => ({
  useVideoPreview: () => ({
    videoSrc: '',
    importId: null,
    isOutOfRange: false,
    videoPreviewMessages: [],
  }),
}))

vi.mock('@/features/widget-preview', () => ({
  WidgetPreview: ({ widget }) => <div>{widget.name}</div>,
  buildMetricWidgetPreviewModel: () => null,
  buildTextWidgetPreviewModel: () => ({
    visualBounds: {
      minX: 0,
      minY: 0,
      maxX: 120,
      maxY: 40,
      width: 120,
      height: 40,
    },
  }),
}))

vi.mock('@/features/widget-preview/hooks/useFontMetricsVersion', () => ({
  useFontMetricsVersion: () => 0,
}))

vi.mock('@/features/widget-preview/utils/textMeasurement', () => ({
  getPreviewFontFamily: (fontFamily) => fontFamily || 'Arial',
}))

vi.mock('@/features/overlay-editor/components/OverlayMoveable', () => ({
  default: () => null,
}))

vi.mock('@/features/overlay-editor/hooks/useEditorViewport', () => ({
  useEditorViewport: () => ({
    viewportRef: { current: null },
    viewportSize: { width: 1920, height: 1080 },
    fitScale: 1,
  }),
}))

function makeLabel(text, overrides = {}) {
  return {
    text,
    x: 0,
    y: 0,
    font_size: 30,
    color: '#ffffff',
    ...overrides,
  }
}

function makeConfig(labels) {
  return {
    ...DEFAULT_CONFIG,
    labels,
    values: [],
    plots: [],
  }
}

describe('OverlayEditor selection flow', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('supports pointer multi-select and delete through the shared selection store', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' })])
    const onConfigChange = vi.fn()

    useStore.getState().setConfig(config)

    const { container } = render(
      <OverlayEditor
        config={config}
        globalDefaults={{ opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        showTemplateStatus={false}
        templateStatus="Saved"
      />,
    )

    const firstWidget = container.querySelector('[data-widget-id="widget-1"]')
    const secondWidget = container.querySelector('[data-widget-id="widget-2"]')

    expect(firstWidget).toBeTruthy()
    expect(secondWidget).toBeTruthy()

    fireEvent.mouseDown(firstWidget, { button: 0 })

    expect(useStore.getState().selectedWidgetIds).toEqual(['widget-1'])
    expect(useStore.getState().selectedWidgetId).toBe('widget-1')

    fireEvent.mouseDown(secondWidget, { button: 0, ctrlKey: true })

    expect(useStore.getState().selectedWidgetIds).toEqual(['widget-1', 'widget-2'])
    expect(useStore.getState().selectedWidgetId).toBe('widget-2')

    fireEvent.keyDown(window, { key: 'Delete' })

    expect(useStore.getState().selectedWidgetIds).toEqual([])
    expect(useStore.getState().selectedWidgetId).toBe(null)
    expect(onConfigChange).toHaveBeenCalledWith(
      expect.objectContaining({
        labels: [],
      }),
    )
  })

  test('copies and pastes the selected widget through the shared config flow', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1', x: 10, y: 20 }), makeLabel('B', { id: 'widget-2', x: 40, y: 50 })])
    const onConfigChange = vi.fn((nextConfig) => {
      useStore.getState().setConfig(nextConfig)
    })

    useStore.getState().setConfig(config)

    const { container } = render(
      <OverlayEditor
        config={useStore.getState().config}
        globalDefaults={{ opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        showTemplateStatus={false}
        templateStatus="Saved"
      />,
    )

    const firstWidget = container.querySelector('[data-widget-id="widget-1"]')

    expect(firstWidget).toBeTruthy()

    fireEvent.mouseDown(firstWidget, { button: 0 })
    fireEvent.keyDown(window, { key: 'c', ctrlKey: true })
    fireEvent.keyDown(window, { key: 'v', ctrlKey: true })

    const state = useStore.getState()
    const labels = state.config.labels

    expect(labels).toHaveLength(3)
    expect(labels[2]).toMatchObject({
      text: 'A',
      x: 34,
      y: 44,
    })
    expect(labels[2].id).not.toBe('widget-1')
    expect(state.selectedWidgetIds).toEqual([labels[2].id])
    expect(state.selectedWidgetId).toBe(labels[2].id)
  })

  test('copies and pastes a multi-selection as newly selected duplicates', () => {
    const config = makeConfig([
      makeLabel('A', { id: 'widget-1', x: 10, y: 20 }),
      makeLabel('B', { id: 'widget-2', x: 40, y: 50 }),
      makeLabel('C', { id: 'widget-3', x: 70, y: 80 }),
    ])
    const onConfigChange = vi.fn((nextConfig) => {
      useStore.getState().setConfig(nextConfig)
    })

    useStore.getState().setConfig(config)

    const { container } = render(
      <OverlayEditor
        config={useStore.getState().config}
        globalDefaults={{ opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        showTemplateStatus={false}
        templateStatus="Saved"
      />,
    )

    const firstWidget = container.querySelector('[data-widget-id="widget-1"]')
    const secondWidget = container.querySelector('[data-widget-id="widget-2"]')

    expect(firstWidget).toBeTruthy()
    expect(secondWidget).toBeTruthy()

    fireEvent.mouseDown(firstWidget, { button: 0 })
    fireEvent.mouseDown(secondWidget, { button: 0, ctrlKey: true })
    fireEvent.keyDown(window, { key: 'c', ctrlKey: true })
    fireEvent.keyDown(window, { key: 'v', ctrlKey: true })

    const state = useStore.getState()
    const labels = state.config.labels

    expect(labels).toHaveLength(5)
    expect(labels.slice(3)).toMatchObject([
      { text: 'A', x: 34, y: 44 },
      { text: 'B', x: 64, y: 74 },
    ])
    expect(state.selectedWidgetIds).toEqual(labels.slice(3).map((label) => label.id))
    expect(state.selectedWidgetId).toBe(labels[4].id)
  })

  test('pastes copied widgets even after the current selection is cleared', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1', x: 10, y: 20 }), makeLabel('B', { id: 'widget-2', x: 40, y: 50 })])
    const onConfigChange = vi.fn((nextConfig) => {
      useStore.getState().setConfig(nextConfig)
    })

    useStore.getState().setConfig(config)

    const { container, getByTestId } = render(
      <OverlayEditor
        config={useStore.getState().config}
        globalDefaults={{ opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        showTemplateStatus={false}
        templateStatus="Saved"
      />,
    )

    const firstWidget = container.querySelector('[data-widget-id="widget-1"]')

    expect(firstWidget).toBeTruthy()

    fireEvent.mouseDown(firstWidget, { button: 0 })
    fireEvent.keyDown(window, { key: 'c', ctrlKey: true })
    fireEvent.mouseDown(getByTestId('overlay-scene'), { button: 0, clientX: 400, clientY: 300 })
    fireEvent.mouseUp(window)

    expect(useStore.getState().selectedWidgetIds).toEqual([])

    fireEvent.keyDown(window, { key: 'v', ctrlKey: true })

    const state = useStore.getState()
    const labels = state.config.labels

    expect(labels).toHaveLength(3)
    expect(state.selectedWidgetIds).toEqual([labels[2].id])
    expect(state.selectedWidgetId).toBe(labels[2].id)
  })

  test('starts marquee selection when the drag begins on the editor stage outside the scene', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' })])

    useStore.getState().setConfig(config)

    const { getByTestId, queryByTestId } = render(
      <OverlayEditor
        config={useStore.getState().config}
        globalDefaults={{ opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        showTemplateStatus={false}
        templateStatus="Saved"
      />,
    )

    expect(queryByTestId('selection-rect')).toBeNull()

    fireEvent.mouseDown(getByTestId('overlay-editor-stage'), { button: 0, clientX: 10, clientY: 10 })
    fireEvent.mouseMove(window, { clientX: 30, clientY: 30 })

    const selectionRect = getByTestId('selection-rect')

    expect(selectionRect).toBeTruthy()
    expect(getByTestId('overlay-editor-stage').contains(selectionRect)).toBe(true)
    expect(getByTestId('overlay-scene').contains(selectionRect)).toBe(false)

    fireEvent.mouseUp(window)
  })
})
