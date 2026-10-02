import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { createLabelDefaults, createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
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
  const snapshot = { activeWidgetInteraction: null, liveWidgetDrafts: {} }
  return {
    beginWidgetInteraction() {},
    clearWidgetDraft() {},
    draftWidgetsRef: { current: {} },
    endWidgetInteraction() {},
    getSnapshot: () => snapshot,
    getWidgetNode: () => ({ dataset: { widgetContentWidth: String(renderedContentWidth) } }),
    setLiveWidgetDraft() {},
    subscribe: () => () => {},
  }
}

describe('useWidgetManager alignment updates', () => {
  test('preserves styles through draft edits and commits only the latest prepared font against current label state', async () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.labels = [{ ...createLabelDefaults(), id: 'label-spacing', letter_spacing: 0, font_weight: 537, italic: true }]
    config.values = [{ ...createMetricValueDefaults('speed', undefined, { displayType: 'linear' }), id: 'speed-0' }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
    useStore.temporal.getState().clear()
    const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits: useWidgetDraftState() }))
    act(() => result.current.updateWidgetSize('label-spacing', { letter_spacing: 2.5 }))
    act(() => result.current.updateWidgetSize('label-spacing', { letter_spacing: -1.25 }))
    expect(useStore.getState().config.labels[0].letter_spacing).toBe(0)
    expect(result.current.widgets.find((widget) => widget.id === 'label-spacing').data.letter_spacing).toBe(-1.25)
    expect(useStore.temporal.getState().pastStates).toHaveLength(0)
    act(() => result.current.commitWidgetSize('label-spacing'))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: -1.25, font_weight: 537, italic: true })
    expect(useStore.temporal.getState().pastStates).toHaveLength(1)
    act(() => undoHistory(useStore))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: 0, font_weight: 537, italic: true })
    act(() => redoHistory(useStore))
    expect(useStore.getState().config.labels[0]).toMatchObject({ letter_spacing: -1.25, font_weight: 537, italic: true })

    act(() => result.current.updateWidgetData('label-spacing', { font: 'Slow.ttf' }))
    act(() => result.current.updateWidgetData('label-spacing', { font: 'Latest.ttf' }))
    act(() => result.current.updateWidgetData('label-spacing', { font_weight: 900 }))
    expect(useStore.getState().config.labels[0].font).not.toBe('Latest.ttf')
    await act(async () => fontRequests.get('Latest.ttf')())
    expect(useStore.getState().config.labels[0]).toMatchObject({ font: 'Latest.ttf', font_weight: 700, italic: false, letter_spacing: -1.25 })
    await act(async () => fontRequests.get('Slow.ttf')())
    expect(useStore.getState().config.labels[0].font).toBe('Latest.ttf')

    act(() => result.current.updateWidgetData('speed-0', { display_variants: { linear: { min_max_label_font: 'Pending.ttf' } } }))
    act(() => result.current.updateWidgetData('speed-0', { display_variants: { linear: { min_max_label_color: '#123456' } } }))
    await act(async () => fontRequests.get('Pending.ttf')())
    expect(useStore.getState().config.values[0].display_variants.linear).toMatchObject({
      min_max_label_font: 'Pending.ttf',
      min_max_label_color: '#123456',
    })

    act(() => result.current.updateWidgetData('label-spacing', { font: 'Obsolete.ttf' }))
    act(() => replaceEditorDocument(useStore, useStore.getState().createNewTemplate))
    await act(async () => fontRequests.get('Obsolete.ttf')())
    expect(useStore.getState().config.labels).toEqual([])
  })
  beforeEach(() => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.values = [{ ...createMetricValueDefaults('speed'), id: 'speed-0', x: 300 }]
    useStore.getState().setConfig(ensureWidgetIdsInConfig(config))
  })

  test('commits the new alignment and compensates x from current rendered geometry', () => {
    const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits: createWidgetLiveEdits(120) }))

    act(() => result.current.updateWidgetData('speed-0', { content_alignment: 'right' }))

    const speed = useStore.getState().config.values.find((value) => value.id === 'speed-0')
    expect(speed.content_alignment).toBe('right')
    expect(speed.x).toBe(420)
  })
})
