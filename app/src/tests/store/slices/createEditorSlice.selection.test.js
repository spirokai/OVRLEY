/**
 * Behavior tests for store-owned overlay selection.
 *
 * These specs document the selection contract after the widget-identity
 * refactor: callers express selection intent once, while the store keeps the
 * ordered selection list and primary selection consistent across config
 * changes using durable widget ids.
 */

import { beforeEach, describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { deleteWidgetInConfig } from '@/lib/widget-config'

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

describe('createEditorSlice selection ownership', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('stores canonical multi-selection order and preferred primary selection', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-3' }), makeLabel('B', { id: 'widget-1' }), makeLabel('C', { id: 'widget-2' })])

    useStore.getState().setConfig(config)
    useStore.getState().setWidgetSelection(['widget-2', 'widget-3'], 'widget-3')

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['widget-3', 'widget-2'])
    expect(state.selectedWidgetId).toBe('widget-3')
  })

  test('keeps the same selected widgets when deleting siblings changes array indexes', () => {
    const config = makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' }), makeLabel('C', { id: 'widget-3' })])

    useStore.getState().setConfig(config)
    useStore.getState().setWidgetSelection(['widget-2', 'widget-3'], 'widget-2')

    useStore.getState().setConfig(deleteWidgetInConfig(config, 'widget-1'))

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['widget-2', 'widget-3'])
    expect(state.selectedWidgetId).toBe('widget-2')
  })

  test('keeps the same selected widgets when config replacement reorders them', () => {
    useStore
      .getState()
      .setConfig(makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' }), makeLabel('C', { id: 'widget-3' })]))
    useStore.getState().setWidgetSelection(['widget-1', 'widget-3'], 'widget-1')

    useStore
      .getState()
      .setConfig(makeConfig([makeLabel('C', { id: 'widget-3' }), makeLabel('B', { id: 'widget-2' }), makeLabel('A', { id: 'widget-1' })]))

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['widget-3', 'widget-1'])
    expect(state.selectedWidgetId).toBe('widget-1')
  })

  test('falls back to the first widget when config replacement invalidates the previous selection', () => {
    useStore.getState().setConfig(makeConfig([makeLabel('A', { id: 'widget-1' }), makeLabel('B', { id: 'widget-2' })]))
    useStore.getState().setWidgetSelection(['widget-2'], 'widget-2')

    useStore.getState().setConfig(makeConfig([makeLabel('X', { id: 'widget-9' }), makeLabel('Y', { id: 'widget-10' })]))

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['widget-9'])
    expect(state.selectedWidgetId).toBe('widget-9')
  })
})
