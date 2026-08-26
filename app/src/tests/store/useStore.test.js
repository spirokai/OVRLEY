import { describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'

describe('useStore layout composition', () => {
  test('exposes the canonical shared-drawer state and actions', () => {
    const state = useStore.getState()

    expect(state).toHaveProperty('activeLeftDrawerTool', 'widgets')
    expect(state).toHaveProperty('leftDrawerPinned')
    expect(state).toHaveProperty('leftDrawerVisible')
    expect(state.selectLeftDrawerTool).toBeTypeOf('function')
    expect(state.dismissLeftDrawerOverlay).toBeTypeOf('function')
    expect(state.setLeftDrawerPinned).toBeTypeOf('function')
  })
})
