import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
/**
 * Preview state and stable WASM canvas attachment across document edits.
 */

import { act, render, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useOverlayEditorStateView from '@/features/overlay-editor/hooks/useOverlayEditorState'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { WasmWidgetCanvas } from '@/features/wasm-widgets/components/WasmWidgetCanvas'
import { presentWidgetGeometry } from '@/features/wasm-widgets/utils/widgetPresentation'
import { COURSE_PLOT_DEFAULTS } from '@/lib/widget/standard-widgets'
import { getPrimarySelectionId, normalizeSelectionIds } from '@/lib/widget/widget-selection'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

function useOverlayEditorState(options) {
  const widgetLiveEdits = useWidgetDraftState()
  const preview = useOverlayEditorStateView({
    editorShell: { activeKeyboardWorkspace: 'editor' },
    gridVisible: false,
    snapToGrid: false,
    zoomLevel: 1,
    onZoomLevelChange: vi.fn(),
    ...options,
    widgetLiveEdits,
  })
  return { ...preview, widgetLiveEdits }
}

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
}

const globalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }

function WasmCanvases({ config, attach }) {
  const state = useOverlayEditorState({ config, globalDefaults, onConfigChange: vi.fn() })
  return state.widgets.map((widget) => (
    <WasmWidgetCanvas key={widget.id} widget={widget} attach={attach} registerNode={state.widgetRefCallbacks[widget.id]} />
  ))
}

describe('useOverlayEditorState module contract', () => {
  beforeEach(resetStore)

  test('keeps existing canvases attached across appearance edits and widget additions', () => {
    const speed = { id: 'speed-widget', value: 'speed', x: 0, y: 0, display_type: 'text', opacity: 1 }
    const config = { ...DEFAULT_CONFIG, values: [speed] }
    const attach = vi.fn()
    const { rerender } = render(<WasmCanvases config={config} attach={attach} />)
    const canvas = attach.mock.calls[0][1]
    expect(attach).toHaveBeenCalledTimes(1)

    const editedSpeed = { ...speed, opacity: 0.5 }
    rerender(<WasmCanvases config={{ ...config, values: [editedSpeed] }} attach={attach} />)
    expect(attach).toHaveBeenCalledTimes(1)

    const otherSpeed = { ...speed, id: 'other-speed' }
    rerender(<WasmCanvases config={{ ...config, values: [editedSpeed, otherSpeed] }} attach={attach} />)
    expect(attach).toHaveBeenCalledTimes(2)
    expect(attach.mock.calls[0]).toEqual(['speed-widget', canvas])

    rerender(<WasmCanvases config={{ ...config, values: [otherSpeed] }} attach={attach} />)
    expect(attach).toHaveBeenCalledTimes(3)
    expect(attach.mock.calls[2]).toEqual(['speed-widget', null])
  })

  test('route rotation centers the selection while its sibling raster stays in scene coordinates', () => {
    const route = { ...COURSE_PLOT_DEFAULTS, id: 'route-widget' }
    const config = { ...DEFAULT_CONFIG, plots: [route] }
    const attach = vi.fn()
    const { rerender, unmount } = render(<WasmCanvases config={config} attach={attach} />)
    const [, canvas, target] = attach.mock.calls[0]
    expect(target.dataset.widgetId).toBe(route.id)
    expect(canvas.parentElement).toBe(target.parentElement)
    for (const rotation of [0, 45, 135]) {
      presentWidgetGeometry(
        canvas,
        {
          left: 90,
          top: 40,
          width: 180,
          height: 150,
          content_width: 160,
          content: { left: 100, top: 60, width: 160, height: 80 },
          rotation,
          rotation_origin: [80, 40],
        },
        target,
      )
      expect(target.style.transformOrigin).toBe('80px 40px')
      expect(target.style.left).toBe('100px')
      expect(target.style.top).toBe('60px')
      expect(canvas.style.left).toBe('90px')
      expect(canvas.style.transform).toBe('none')
    }
    rerender(<WasmCanvases config={{ ...config, plots: [{ ...route, rotation: 90 }] }} attach={attach} />)
    expect(attach).toHaveBeenCalledTimes(1)
    unmount()
    expect(attach.mock.lastCall[1]).toBeNull()
  })

  test('keeps node registration stable across display changes', () => {
    const speed = { id: 'speed-widget', value: 'speed', x: 0, y: 0, display_type: 'text' }
    const { result, rerender } = renderHook(({ config }) => useOverlayEditorState({ config, globalDefaults, onConfigChange: vi.fn() }), {
      initialProps: { config: { ...DEFAULT_CONFIG, values: [speed] } },
    })
    const wasmRef = result.current.widgetRefCallbacks['speed-widget']
    const node = document.createElement('canvas')
    act(() => wasmRef(node))
    expect(result.current.widgetLiveEdits.getWidgetNode('speed-widget')).toBe(node)

    rerender({ config: { ...DEFAULT_CONFIG, values: [{ ...speed, display_type: 'linear' }] } })
    const linearRef = result.current.widgetRefCallbacks['speed-widget']
    expect(linearRef).toBe(wasmRef)
    act(() => {
      wasmRef(null)
      linearRef(node)
    })
    expect(result.current.widgetLiveEdits.getWidgetNode('speed-widget')).toBe(node)
  })

  test('keeps widgets and interaction refs available after the playhead leaves activity data', () => {
    const config = {
      ...DEFAULT_CONFIG,
      values: [{ id: 'speed-widget', value: 'speed', x: 0, y: 0, display_type: 'text' }],
    }
    useStore.setState({
      fallbackDurationSeconds: 10,
      parsedActivity: null,
      selectedSecond: 9,
    })

    const { result } = renderHook(() =>
      useOverlayEditorState({
        config,
        globalDefaults: { ...DEFAULT_GLOBAL_DEFAULTS },
        onConfigChange: vi.fn(),
        zoomLevel: 1,
        onZoomLevelChange: vi.fn(),
      }),
    )

    expect(result.current.previewSecond).toBe(9)
    expect(result.current.widgets).toHaveLength(1)
    expect(result.current.widgetRefCallbacks['speed-widget']).toEqual(expect.any(Function))

    act(() => {
      useStore.setState({
        parsedActivity: {
          trim_end_seconds: 3,
          metadata: { duration_seconds: 3 },
        },
      })
    })

    expect(result.current.previewSecond).toBe(9)
    expect(result.current.activity).toEqual({
      trim_end_seconds: 3,
      metadata: { duration_seconds: 3 },
    })
    expect(result.current.widgets).toHaveLength(1)
    expect(result.current.widgetRefCallbacks['speed-widget']).toEqual(expect.any(Function))
  })
})

describe('useOverlayEditorState dependencies — selection helpers', () => {
  test('normalizeSelectionIds returns only valid IDs from the ordered list', () => {
    const result = normalizeSelectionIds(['w1', 'w3', 'ghost'], ['w1', 'w2', 'w3'])
    expect(result).toEqual(['w1', 'w3'])
  })

  test('normalizeSelectionIds handles empty input', () => {
    expect(normalizeSelectionIds([], ['w1', 'w2'])).toEqual([])
  })

  test('getPrimarySelectionId picks the last ID when preferred is null', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], null)).toBe('w2')
  })

  test('getPrimarySelectionId uses preferred ID when it exists in the list', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], 'w1')).toBe('w1')
  })

  test('getPrimarySelectionId falls back to last when preferred is not in list', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], 'ghost')).toBe('w2')
  })

  test('getPrimarySelectionId returns null for empty list', () => {
    expect(getPrimarySelectionId([], 'w1')).toBeNull()
  })
})
