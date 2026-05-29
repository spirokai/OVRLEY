// @vitest-environment node

/**
 * Node-environment guard for store imports.
 *
 * This spec proves that store creation no longer assumes `window` or
 * `localStorage` exist. It protects the non-browser startup contract added by
 * the storage-hydration refactor.
 */

import { describe, expect, test, vi } from 'vitest'

describe('useStore without browser globals', () => {
  test('imports and creates state without window or localStorage', async () => {
    // Force a fresh module evaluation in the actual node environment rather
    // than reusing a store instance that may have been created elsewhere.
    vi.resetModules()

    const module = await import('@/store/useStore')
    const state = module.default.getState()

    expect(state.config).toBeDefined()
    expect(state.globalDefaults).toBeDefined()
    expect(state.startSecond).toBe(0)
    expect(state.loadedTemplateFilename).toBeNull()
  })
})
