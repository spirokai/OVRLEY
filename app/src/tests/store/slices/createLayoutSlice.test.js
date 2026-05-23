/**
 * Tests for createLayoutSlice — verifies drawer open/close state management.
 */

import { describe, test, expect } from 'vitest'
import { createLayoutSlice } from '@/store/slices/createLayoutSlice'

function createMockSet() {
  const state = {}
  const set = (fn) => {
    fn(state)
  }
  return { state, set }
}

describe('createLayoutSlice', () => {
  test('widgetDrawerOpen defaults to false', () => {
    const { set } = createMockSet()
    const get = () => ({})

    const slice = createLayoutSlice(set, get)

    expect(slice.widgetDrawerOpen).toBe(false)
  })

  test('toggleWidgetDrawer opens the drawer when closed', () => {
    const { state, set } = createMockSet()
    const get = () => state

    const slice = createLayoutSlice(set, get)

    slice.toggleWidgetDrawer()

    expect(state.widgetDrawerOpen).toBe(true)
  })

  test('toggleWidgetDrawer closes the drawer when open', () => {
    const { state, set } = createMockSet()
    state.widgetDrawerOpen = true
    const get = () => state

    const slice = createLayoutSlice(set, get)

    slice.toggleWidgetDrawer()

    expect(state.widgetDrawerOpen).toBe(false)
  })
})
