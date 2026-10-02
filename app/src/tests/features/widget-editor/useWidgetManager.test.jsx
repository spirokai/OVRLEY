import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { createLabelDefaults, createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { redoHistory, undoHistory } from '@/features/undo-redo/undoHistory'
import { ensureWidgetIdsInConfig } from '@/lib/widget/widget-config'
import useStore from '@/store/useStore'
import { cloneSerializable, DEFAULT_CONFIG } from '@/store/store-utils'

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
  test('previews spacing in drafts and commits one undoable edit while preserving label styles', () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.labels = [{ ...createLabelDefaults(), id: 'label-spacing', letter_spacing: 0, font_weight: 537, italic: true }]
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
