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

vi.mock('@/lib/activity/cache', () => ({
  getCurrentParsedActivity: () => null,
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
})
