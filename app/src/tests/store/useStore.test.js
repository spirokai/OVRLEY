/**
 * Tests for store composition — verifies layout slice is composed into the store.
 */

import { describe, test, expect } from 'vitest'
import useStore from '@/store/useStore'

describe('useStore — layout slice composition', () => {
  test('store exposes widgetDrawerOpen state', () => {
    const state = useStore.getState()

    expect(state).toHaveProperty('widgetDrawerOpen')
    expect(state.widgetDrawerOpen).toBe(false)
  })

  test('store exposes toggleWidgetDrawer action', () => {
    const state = useStore.getState()

    expect(state).toHaveProperty('toggleWidgetDrawer')
    expect(typeof state.toggleWidgetDrawer).toBe('function')
  })

  test('store exposes closeWidgetDrawer action', () => {
    const state = useStore.getState()

    expect(state).toHaveProperty('closeWidgetDrawer')
    expect(typeof state.closeWidgetDrawer).toBe('function')
  })

  test('toggleWidgetDrawer toggles widgetDrawerOpen in the store', () => {
    useStore.getState().toggleWidgetDrawer()

    expect(useStore.getState().widgetDrawerOpen).toBe(true)

    useStore.getState().toggleWidgetDrawer()

    expect(useStore.getState().widgetDrawerOpen).toBe(false)
  })

  test('closeWidgetDrawer closes widgetDrawerOpen in the store', () => {
    useStore.getState().toggleWidgetDrawer()

    expect(useStore.getState().widgetDrawerOpen).toBe(true)

    useStore.getState().closeWidgetDrawer()

    expect(useStore.getState().widgetDrawerOpen).toBe(false)
  })
})
