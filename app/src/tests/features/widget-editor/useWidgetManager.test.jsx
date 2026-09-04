import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'
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
