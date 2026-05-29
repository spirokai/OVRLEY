/**
 * Behavior tests for store-owned overlay selection.
 *
 * These specs document the selection contract after the ownership refactor:
 * callers express selection intent once, while the store keeps the ordered
 * selection list and primary selection consistent across config changes.
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
    const config = makeConfig([makeLabel('A'), makeLabel('B'), makeLabel('C')])

    useStore.getState().setConfig(config)
    useStore.getState().setWidgetSelection(['label-2', 'label-0'], 'label-0')

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['label-0', 'label-2'])
    expect(state.selectedWidgetId).toBe('label-0')
  })

  test('remaps selected widgets when config deletion shifts widget indexes', () => {
    const first = makeLabel('A')
    const second = makeLabel('B')
    const third = makeLabel('C')
    const config = makeConfig([first, second, third])

    useStore.getState().setConfig(config)
    useStore.getState().setWidgetSelection(['label-1', 'label-2'], 'label-1')

    useStore.getState().setConfig(deleteWidgetInConfig(config, 'label-0'))

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['label-0', 'label-1'])
    expect(state.selectedWidgetId).toBe('label-0')
  })

  test('falls back to the first widget when config replacement invalidates the previous selection', () => {
    useStore.getState().setConfig(makeConfig([makeLabel('A'), makeLabel('B')]))
    useStore.getState().setWidgetSelection(['label-1'], 'label-1')

    useStore.getState().setConfig(makeConfig([makeLabel('X'), makeLabel('Y')]))

    const state = useStore.getState()

    expect(state.selectedWidgetIds).toEqual(['label-0'])
    expect(state.selectedWidgetId).toBe('label-0')
  })
})
