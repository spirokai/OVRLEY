/**
 * Characterizes the dev-mode window leak from the zustand store.
 * After refactoring, only __OVRLEY_STORE__ should remain.
 */
import { describe, expect, test, vi } from 'vitest'

describe('useStore window exposure', () => {
  test('exposes exactly one store reference on window in dev mode', async () => {
    delete window.useStore
    delete window.__OVRLEY_STORE__
    delete window.__STORE__

    vi.stubEnv('DEV', true)
    const { default: _useStore } = await import('@/store/useStore')

    expect(window.__OVRLEY_STORE__).toBeDefined()
    expect(window.useStore).toBeUndefined()
    expect(window.__STORE__).toBeUndefined()
  })
})
