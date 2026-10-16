import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
/**
 * Integration tests for overlay-editor selection behavior.
 *
 * The editor should drive selection through pointer intents while the shared
 * store remains the single owner of the selected-id list and primary widget.
 */

import { act, fireEvent, render, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import OverlayEditorView from '@/features/overlay-editor/components/OverlayEditor'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

const moveableUpdateRectMock = vi.fn()
function OverlayEditor(props) {
  const widgetLiveEdits = useWidgetDraftState()
  return (
    <OverlayEditorView
      {...props}
      editorShell={props.editorShell ?? defaultEditorShell}
      undoRedoControls={props.undoRedoControls ?? defaultUndoRedoControls}
      widgetLiveEdits={widgetLiveEdits}
    />
  )
}

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

vi.mock('@/features/overlay-editor/components/OverlayMoveable', () => ({
  default: function MockOverlayMoveable({ canResizeSelected, maintainAspectRatio, moveableRef, showEdgeResizeHandles }) {
    moveableRef.current = { updateRect: moveableUpdateRectMock }

    return (
      <div
        data-testid="moveable-props"
        data-can-resize={String(canResizeSelected)}
        data-maintain-ratio={String(maintainAspectRatio)}
        data-edge-handles={String(showEdgeResizeHandles)}
      />
    )
  },
}))

vi.mock('@/features/overlay-editor/hooks/useEditorViewport', () => ({
  useEditorViewport: () => ({
    viewportRef: { current: null },
    viewportSize: { width: 1920, height: 1080 },
    fitScale: 1,
    displayScale: 1,
    handleWheel: vi.fn(),
    scrollViewportRef: { current: null },
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

const defaultUndoRedoControls = { canRedo: false, canUndo: false, redo: vi.fn(), undo: vi.fn() }

const defaultEditorShell = {
  activeKeyboardWorkspace: 'editor',
  decreaseZoom: vi.fn(),
  editorBackgroundMode: 'black',
  editorGridVisible: false,
  editorSnapToGrid: false,
  editorZoomLevel: 1,
  increaseZoom: vi.fn(),
  resetZoom: vi.fn(),
  setEditorBackgroundMode: vi.fn(),
  setEditorGridVisible: vi.fn(),
  setEditorSnapToGrid: vi.fn(),
}

describe('OverlayEditor selection flow', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    moveableUpdateRectMock.mockReset()
    vi.stubGlobal('requestAnimationFrame', (callback) => {
      callback(0)
      return 1
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})
  })

  test('supports pointer multi-select and delete through the shared selection store', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' })])
    const onConfigChange = vi.fn()

    useStore.getState().setConfig(config)

    const { container } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
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
    expect(container.querySelector('[data-testid="moveable-props"]')).toHaveAttribute('data-can-resize', 'true')
    expect(container.querySelector('[data-testid="moveable-props"]')).toHaveAttribute('data-maintain-ratio', 'true')

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
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
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
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
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
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={onConfigChange}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
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
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
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

  test.each(['backdrop', 'lean_angle'])('enables the supported resize handles for selected %s widgets', (type) => {
    const config = {
      ...DEFAULT_CONFIG,
      backdrops: type === 'backdrop' ? [{ ...createWidgetDefaults({ type: 'backdrop', displayType: 'rectangle' }), id: 'widget-resize' }] : [],
      labels: [],
      plots: [],
      values:
        type === 'lean_angle' ? [{ ...createWidgetDefaults({ type: 'lean_angle', displayType: 'lean_angle' }, undefined), id: 'widget-resize' }] : [],
    }

    useStore.getState().setConfig(config)

    const { container, getByTestId } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
      />,
    )

    const target = container.querySelector('[data-widget-id="widget-resize"]')
    expect(target).toBeTruthy()

    fireEvent.mouseDown(target, { button: 0 })

    expect(getByTestId('moveable-props')).toHaveAttribute('data-can-resize', 'true')
    expect(getByTestId('moveable-props')).toHaveAttribute('data-maintain-ratio', String(type === 'lean_angle'))
    expect(getByTestId('moveable-props')).toHaveAttribute('data-edge-handles', 'false')
  })

  test('maintains a square frame for selected arc widgets', () => {
    const config = {
      ...DEFAULT_CONFIG,
      backdrops: [],
      labels: [],
      plots: [],
      values: [
        {
          id: 'widget-arc',
          value: 'speed',
          x: 0,
          y: 0,
          display_type: 'arc',
          display_variants: {
            arc: {
              width: 220,
              height: 220,
            },
          },
        },
      ],
    }

    useStore.getState().setConfig(config)

    const { container, getByTestId } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
      />,
    )

    const arc = container.querySelector('[data-widget-id="widget-arc"]')
    expect(arc).toBeTruthy()

    fireEvent.mouseDown(arc, { button: 0 })

    expect(getByTestId('moveable-props')).toHaveAttribute('data-can-resize', 'true')
    expect(getByTestId('moveable-props')).toHaveAttribute('data-maintain-ratio', 'true')
  })

  test('uses corner-only square resize handles for selected G-force widgets', () => {
    const config = {
      ...DEFAULT_CONFIG,
      backdrops: [],
      labels: [],
      plots: [],
      values: [
        {
          id: 'widget-g-force',
          value: 'g_force',
          x: 0,
          y: 0,
          display_type: 'g_force',
          display_variants: {
            g_force: {
              width: 220,
              height: 220,
              diameter: 200,
            },
          },
        },
      ],
    }

    useStore.getState().setConfig(config)

    const { container, getByTestId } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
      />,
    )

    const gForce = container.querySelector('[data-widget-id="widget-g-force"]')
    expect(gForce).toBeTruthy()

    fireEvent.mouseDown(gForce, { button: 0 })

    expect(getByTestId('moveable-props')).toHaveAttribute('data-can-resize', 'true')
    expect(getByTestId('moveable-props')).toHaveAttribute('data-maintain-ratio', 'true')
    expect(getByTestId('moveable-props')).toHaveAttribute('data-edge-handles', 'false')
  })

  test('renders the canvas toolbar centered above the preview area', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1' })])

    useStore.getState().setConfig(config)

    const { getByTestId } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus
        projectStatus="Modified"
      />,
    )

    const badgeRow = getByTestId('canvas-status-badges')
    const toolbar = getByTestId('canvas-editor-toolbar')

    expect(within(badgeRow).getByText(/1920 .* 1080/)).toBeTruthy()
    expect(within(badgeRow).getByText('Modified')).toBeTruthy()
    expect(within(toolbar).getByText('100%')).toBeTruthy()
    expect(toolbar.className).toContain('left-1/2')
    expect(toolbar.className).toContain('-translate-x-1/2')
  })

  test('refreshes moveable bounds when WASM publishes the selected widget geometry', async () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1', font_size: 30 })])
    const nextConfig = makeConfig([makeLabel('A', { id: 'widget-1', font_size: 60 })])

    useStore.getState().setConfig(config)

    const { container, rerender } = render(
      <OverlayEditor
        config={config}
        editorShell={defaultEditorShell}
        globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
        onConfigChange={vi.fn()}
        zoomLevel={1}
        onZoomLevelChange={vi.fn()}
        backgroundMode="black"
        gridVisible={false}
        snapToGrid={false}
        importedBackgroundImageFilename={null}
        importedVideoFilename={null}
        showProjectStatus={false}
        projectStatus="Saved"
      />,
    )

    const widget = container.querySelector('[data-widget-id="widget-1"]')
    expect(widget).toBeTruthy()

    fireEvent.mouseDown(widget, { button: 0 })
    moveableUpdateRectMock.mockClear()

    act(() => {
      useStore.getState().setConfig(nextConfig)
      rerender(
        <OverlayEditor
          config={nextConfig}
          editorShell={defaultEditorShell}
          globalDefaults={{ ...DEFAULT_GLOBAL_DEFAULTS, opacity: 1, scale: 1 }}
          onConfigChange={vi.fn()}
          zoomLevel={1}
          onZoomLevelChange={vi.fn()}
          backgroundMode="black"
          gridVisible={false}
          snapToGrid={false}
          importedBackgroundImageFilename={null}
          importedVideoFilename={null}
          showProjectStatus={false}
          projectStatus="Saved"
        />,
      )
    })

    await act(async () => {
      widget.dispatchEvent(new Event('wasm-widget-render', { bubbles: true }))
    })
    expect(moveableUpdateRectMock).toHaveBeenCalledTimes(1)
  })
})
