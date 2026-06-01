/**
 * Regression tests for config/timeline synchronization through the public store API.
 *
 * These tests document the supported in-session synchronization contract:
 *
 * - Config-originated runtime updates hydrate timeline state immediately.
 * - Template hydration does not import activity-specific scene timing.
 * - Timeline-originated edits write back into config scene timing immediately.
 * - Mixed update ordering must work without relying on timer windows.
 */

import { beforeEach, describe, expect, test, vi } from 'vitest'
import { cloneSerializable, DEFAULT_CONFIG } from '@/store/store-utils'

/**
 * Re-imports the store after resetting the module graph so each test gets a
 * fresh Zustand store instance.
 *
 * @returns {Promise<import('@/store/useStore').default>} Fresh Zustand store instance.
 */
async function loadFreshStore() {
  vi.resetModules()
  const module = await import('@/store/useStore')
  return module.default
}

describe('config/timeline synchronization', () => {
  let useStore

  beforeEach(async () => {
    useStore = await loadFreshStore()
  })

  test('config replacement hydrates timeline state and immediate timeline edits sync back into config', () => {
    const nextConfig = cloneSerializable(DEFAULT_CONFIG)
    nextConfig.scene.start = 5
    nextConfig.scene.end = 90

    useStore.getState().setConfig(nextConfig)

    expect(useStore.getState().startSecond).toBe(5)
    expect(useStore.getState().endSecond).toBe(90)
    expect(useStore.getState().selectedSecond).toBe(5)

    useStore.getState().setStartSecond(7)
    useStore.getState().setEndSecond(88)

    const state = useStore.getState()
    expect(state.startSecond).toBe(7)
    expect(state.endSecond).toBe(88)
    expect(state.config.scene.start).toBe(7)
    expect(state.config.scene.end).toBe(88)
  })

  test('template hydration strips scene timing, then immediate timeline edits still sync back into config', () => {
    const templateConfig = cloneSerializable(DEFAULT_CONFIG)
    templateConfig.scene.start = 12
    templateConfig.scene.end = 144

    useStore.getState().hydrateTemplateState(
      {
        config: templateConfig,
        settings: { globalDefaults: { color_text: '#abcdef' } },
      },
      { filename: 'template.json', source: 'file' },
    )

    expect(useStore.getState().config.scene).not.toHaveProperty('start')
    expect(useStore.getState().config.scene).not.toHaveProperty('end')
    expect(useStore.getState().startSecond).toBe(0)
    expect(useStore.getState().endSecond).toBe(73)
    expect(useStore.getState().selectedSecond).toBe(0)

    useStore.getState().setStartSecond(14)
    useStore.getState().setEndSecond(140)

    const state = useStore.getState()
    expect(state.config.scene.start).toBe(14)
    expect(state.config.scene.end).toBe(140)
  })

  test('rapid consecutive timeline edits keep config scene timing synchronized without timer windows', () => {
    useStore.getState().setStartSecond(1)
    expect(useStore.getState().config.scene.start).toBe(1)

    useStore.getState().setStartSecond(2)
    expect(useStore.getState().config.scene.start).toBe(2)

    useStore.getState().setStartSecond(3)
    expect(useStore.getState().config.scene.start).toBe(3)

    useStore.getState().setEndSecond(72)
    expect(useStore.getState().config.scene.end).toBe(72)

    useStore.getState().setEndSecond(71)
    expect(useStore.getState().config.scene.end).toBe(71)

    useStore.getState().setEndSecond(70)

    const state = useStore.getState()
    expect(state.startSecond).toBe(3)
    expect(state.endSecond).toBe(70)
    expect(state.config.scene.start).toBe(3)
    expect(state.config.scene.end).toBe(70)
  })

  test('dirty tracking recalculates across mixed config replacement and timeline edits', () => {
    useStore.getState().setLastRenderedConfig(cloneSerializable(DEFAULT_CONFIG))

    const changedConfig = cloneSerializable(DEFAULT_CONFIG)
    changedConfig.scene.start = 4
    useStore.getState().setConfig(changedConfig)

    expect(useStore.getState().hasUnrenderedChanges).toBe(true)

    useStore.getState().setStartSecond(0)

    const state = useStore.getState()
    expect(state.config.scene.start).toBe(0)
    expect(state.hasUnrenderedChanges).toBe(false)
  })

  test('widget-only config edits preserve the current playhead progress', () => {
    useStore.getState().setSelectedSecond(27)

    const nextConfig = cloneSerializable(useStore.getState().config)
    nextConfig.scene.font_size = 42
    useStore.getState().setConfig(nextConfig)

    const state = useStore.getState()
    expect(state.selectedSecond).toBe(27)
    expect(state.startSecond).toBe(0)
    expect(state.endSecond).toBe(73)
    expect(state.config.scene.font_size).toBe(42)
  })
})
