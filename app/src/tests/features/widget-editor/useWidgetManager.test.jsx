import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'

import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { registerWidgetInteraction, unregisterWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { redoHistory, replaceEditorDocument, undoHistory } from '@/features/undo-redo/undoHistory'
import { ensureWidgetIdsInConfig } from '@/lib/widget/widget-config'
import useStore from '@/store/useStore'
import { cloneSerializable, DEFAULT_CONFIG } from '@/store/store-utils'

const fontRequests = vi.hoisted(() => new Map())
vi.mock('@/lib/font-resources', () => ({
  isFontPrepared: () => false,
  getPreparedFont: (id) => ({ id, name: id, faces: [{ style: 'normal', weight: 700, axes: [] }] }),
  prepareFont: vi.fn((id) => new Promise((resolve) => fontRequests.set(id, resolve))),
}))

function createWidgetLiveEdits(renderedContentWidth) {
  const snapshot = { liveWidgetDrafts: {}, widgetNodes: {} }
  return {
    clearWidgetDraft() {},
    getSnapshot: () => snapshot,
    getWidgetNode: () => ({ dataset: { widgetContentWidth: String(renderedContentWidth) } }),
    setLiveWidgetDraft() {},
    subscribe: () => () => {},
  }
}

describe('useWidgetManager alignment updates', () => {
  test('preserves label drafts and commits WASM font edits without browser font preparation', async () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.labels = [{ ...createWidgetDefaults({ type: 'label' }), id: 'label-spacing', letter_spacing: 0, font_weight: 537, italic: true }]
    config.values = [{ ...createWidgetDefaults({ type: 'speed', displayType: 'linear' }, undefined), id: 'speed-0' }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
    useStore.temporal.getState().clear()
    const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits: useWidgetDraftState() }))
    act(() => result.current.updateWidgetDraft('label-spacing', { letter_spacing: 2.5 }))
    act(() => result.current.updateWidgetDraft('label-spacing', { letter_spacing: -1.25 }))
    expect(useStore.getState().config.labels[0].letter_spacing).toBe(0)
    await waitFor(() => expect(result.current.widgets.find((widget) => widget.id === 'label-spacing').data.letter_spacing).toBe(-1.25))
    expect(useStore.temporal.getState().pastStates).toHaveLength(0)
    act(() => result.current.commitWidgetDraft('label-spacing'))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: -1.25, font_weight: 537, italic: true })
    expect(useStore.temporal.getState().pastStates).toHaveLength(1)
    act(() => undoHistory(useStore))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: 0, font_weight: 537, italic: true })
    act(() => redoHistory(useStore))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: -1.25, font_weight: 537, italic: true })

    act(() => result.current.updateWidgetData('label-spacing', { font: 'Slow.ttf' }))
    act(() => result.current.updateWidgetData('label-spacing', { font: 'Latest.ttf' }))
    act(() => result.current.updateWidgetData('label-spacing', { font_weight: 900 }))
    expect(useStore.getState().config.labels[0]).toMatchObject({ font: 'Latest.ttf', font_weight: 900, italic: true, letter_spacing: -1.25 })
    expect(fontRequests.has('Latest.ttf')).toBe(false)
    expect(fontRequests.has('Slow.ttf')).toBe(false)

    act(() => result.current.updateWidgetData('speed-0', { display_variants: { linear: { min_max_label_font: 'Pending.ttf' } } }))
    act(() => result.current.updateWidgetData('speed-0', { display_variants: { linear: { min_max_label_color: '#123456' } } }))
    expect(fontRequests.has('Pending.ttf')).toBe(false)
    expect(useStore.getState().config.values[0].display_variants.linear).toMatchObject({
      min_max_label_font: 'Pending.ttf',
      min_max_label_color: '#123456',
    })

    act(() => result.current.updateWidgetData('label-spacing', { font: 'Obsolete.ttf' }))
    act(() => replaceEditorDocument(useStore, useStore.getState().createNewTemplate))
    expect(fontRequests.has('Obsolete.ttf')).toBe(false)
    expect(useStore.getState().config.labels).toEqual([])
  })
  beforeEach(() => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.values = [{ ...createWidgetDefaults({ type: 'speed' }), id: 'speed-0', x: 300 }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
  })

  test.each([
    ['speed', 'text'],
    ['time', 'text'],
    ['gradient', 'text'],
    ['lean_angle', 'lean_angle'],
    ['lap_timer', 'lap_timer'],
    ['heading', 'heading_tape'],
    ['speed', 'linear'],
    ['speed', 'arc'],
    ['speed', 'corner'],
  ])('%s/%s font sizes commit on release before the pending frame', (type, displayType) => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.values = [
      {
        ...createWidgetDefaults({ type: type, displayType, lapTimerMode: 'current_lap' }, undefined),
        id: 'speed-0',
      },
    ]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
    vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
    try {
      useStore.temporal.getState().clear()
      const initialSize = useStore.getState().config.values[0].font_size
      const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits: useWidgetDraftState() }))

      act(() => result.current.updateWidgetData('speed-0', { font: 'Immediate.ttf' }))
      expect(useStore.getState().config.values[0].font).toBe('Immediate.ttf')
      expect(fontRequests.has('Immediate.ttf')).toBe(false)
      if (type === 'lap_timer') {
        act(() => result.current.updateWidgetData('speed-0', { label_font: 'ImmediateLabel.ttf' }))
        expect(useStore.getState().config.values[0].label_font).toBe('ImmediateLabel.ttf')
        expect(fontRequests.has('ImmediateLabel.ttf')).toBe(false)
      }
      if (type === 'heading') {
        act(() => result.current.updateWidgetData('speed-0', { display_variants: { heading_tape: { label_font: 'ImmediateLabel.ttf' } } }))
        expect(useStore.getState().config.values[0].display_variants.heading_tape.label_font).toBe('ImmediateLabel.ttf')
        expect(fontRequests.has('ImmediateLabel.ttf')).toBe(false)
      }
      if (['linear', 'arc', 'corner'].includes(displayType)) {
        act(() => result.current.updateWidgetData('speed-0', { display_variants: { [displayType]: { min_max_label_font: 'ImmediateRange.ttf' } } }))
        expect(useStore.getState().config.values[0].display_variants[displayType].min_max_label_font).toBe('ImmediateRange.ttf')
        expect(fontRequests.has('ImmediateRange.ttf')).toBe(false)
      }
      useStore.temporal.getState().clear()

      act(() => {
        result.current.updateWidgetDraft('speed-0', { font_size: 50 })
        result.current.updateWidgetDraft('speed-0', { font_size: 60.375 })
        if (type === 'lap_timer') result.current.updateWidgetDraft('speed-0', { label_font_size: 17.125 })
        if (type === 'heading') result.current.updateWidgetDraft('speed-0', { display_variants: { heading_tape: { label_font_size: 17.125 } } })
      })
      expect(useStore.getState().config.values[0].font_size).toBe(initialSize)
      expect(vi.getTimerCount()).toBe(1)

      act(() => result.current.commitWidgetDraft('speed-0'))
      expect(useStore.getState().config.values[0].font_size).toBe(60.375)
      expect(result.current.widgets[0].data.font_size).toBe(60.375)
      if (type === 'lap_timer') expect(result.current.widgets[0].data.label_font_size).toBe(17.125)
      if (type === 'heading') expect(result.current.widgets[0].data.display_variants.heading_tape.label_font_size).toBe(17.125)
      expect(useStore.temporal.getState().pastStates).toHaveLength(1)
      expect(vi.getTimerCount()).toBe(0)
      act(() => vi.advanceTimersToNextFrame())
      expect(result.current.widgets[0].data.font_size).toBe(60.375)
    } finally {
      vi.useRealTimers()
    }
  })

  test('course rotation slider uses the canvas controller and commits its exact patch', () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.plots = [{ ...WIDGET_TYPE_DEFINITIONS.course.defaults, id: 'route-0' }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
    const node = document.createElement('div')
    const patch = { x: 123.125, y: 80.25, rotation: 314.75 }
    const rotate = vi.fn(() => patch)
    registerWidgetInteraction(node, { rotate })
    const { result } = renderHook(() => {
      const liveEdits = useWidgetDraftState()
      return { manager: useWidgetManager({ widgetLiveEdits: liveEdits }), liveEdits }
    })
    result.current.liveEdits.getWidgetNodeRef('route-0')(node)
    act(() => result.current.manager.updateWidgetDraft('route-0', { rotation: -45.25 }))
    expect(rotate).toHaveBeenCalledWith(-45.25)
    act(() => result.current.manager.commitWidgetDraft('route-0'))
    expect(useStore.getState().config.plots[0]).toMatchObject(patch)
    unregisterWidgetInteraction(node)
  })

  test('heading offset commits the WASM height response before the pending frame', () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.values = [{ ...createWidgetDefaults({ type: 'heading', displayType: 'heading_tape' }, undefined), id: 'heading-0' }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
    const originalTape = useStore.getState().config.values[0].display_variants.heading_tape
    const node = document.createElement('div')
    const resize = vi.fn().mockReturnValue({ display_variants: { heading_tape: { height: 96.125, label_offset: 9.125 } } })
    registerWidgetInteraction(node, { resize })
    vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
    try {
      const { result } = renderHook(() => {
        const liveEdits = useWidgetDraftState()
        return { manager: useWidgetManager({ widgetLiveEdits: liveEdits }), liveEdits }
      })
      result.current.liveEdits.getWidgetNodeRef('heading-0')(node)
      useStore.temporal.getState().clear()
      act(() => result.current.manager.updateWidgetDraft('heading-0', { display_variants: { heading_tape: { label_offset: 9.125 } } }))
      expect(resize).toHaveBeenCalledWith({ label_offset: 9.125 })
      expect(useStore.getState().config.values[0].display_variants.heading_tape).toEqual(originalTape)
      act(() => result.current.manager.commitWidgetDraft('heading-0'))
      expect(useStore.getState().config.values[0].display_variants.heading_tape).toEqual({ ...originalTape, height: 96.125, label_offset: 9.125 })
      expect(result.current.manager.widgets[0].data.display_variants.heading_tape.height).toBe(96.125)
      expect(useStore.temporal.getState().pastStates).toHaveLength(1)
      expect(vi.getTimerCount()).toBe(0)
    } finally {
      unregisterWidgetInteraction(node)
      vi.useRealTimers()
    }
  })

  test('commits the new alignment and compensates x from current rendered geometry', () => {
    const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits: createWidgetLiveEdits(120) }))

    act(() => result.current.updateWidgetData('speed-0', { content_alignment: 'right' }))

    const speed = useStore.getState().config.values.find((value) => value.id === 'speed-0')
    expect(speed.content_alignment).toBe('right')
    expect(speed.x).toBe(420)
  })
})
