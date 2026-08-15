import { describe, expect, test } from 'vitest'
import { createLayoutSlice } from '@/store/slices/createLayoutSlice'

function createLayoutHarness() {
  const state = {}
  const set = (update) => update(state)
  const slice = createLayoutSlice(set, () => state)
  Object.assign(state, slice)
  return state
}

describe('createLayoutSlice', () => {
  test('initialization restores a pinned drawer visibly with its active tool', () => {
    const state = createLayoutHarness()

    state.initializeLeftDrawer({ pinned: true, activeTool: 'widgets' })

    expect(state).toMatchObject({
      activeLeftDrawerTool: 'widgets',
      leftDrawerInitialized: true,
      leftDrawerPinned: true,
      leftDrawerVisible: true,
    })
  })

  test('selecting the active tool toggles only an unpinned drawer', () => {
    const state = createLayoutHarness()
    state.initializeLeftDrawer({ pinned: false, activeTool: 'widgets' })

    state.selectLeftDrawerTool('widgets')
    expect(state.leftDrawerVisible).toBe(true)

    state.selectLeftDrawerTool('widgets')
    expect(state.leftDrawerVisible).toBe(false)

    state.setLeftDrawerPinned(true)
    state.selectLeftDrawerTool('widgets')
    expect(state.leftDrawerVisible).toBe(true)
  })

  test('dismissal and unpinning preserve the canonical visibility rules', () => {
    const state = createLayoutHarness()
    state.initializeLeftDrawer({ pinned: true, activeTool: 'widgets' })

    state.dismissLeftDrawerOverlay()
    expect(state.leftDrawerVisible).toBe(true)

    state.setLeftDrawerPinned(false)
    expect(state).toMatchObject({ leftDrawerPinned: false, leftDrawerVisible: true })

    state.dismissLeftDrawerOverlay()
    expect(state.leftDrawerVisible).toBe(false)
  })
})
