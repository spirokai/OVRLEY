/**
 * Startup regression tests for the app store.
 *
 * These tests lock in the new durability rule introduced by the bad-practice
 * remediation work:
 *
 * - Launch starts from explicit in-memory defaults.
 * - Restarting after a session reset returns to those same defaults.
 * - Importing the store and editor-shell modules is storage-pure.
 * - Explicit template loading still updates active state, but only in memory.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template-state'
import { createEditorEffectiveConfig } from '@/lib/template-state'
import { DEFAULT_CONFIG } from '@/store/store-utils'

/**
 * Re-imports the store after resetting the module graph.
 *
 * The startup behavior under test happens during module evaluation and store
 * creation, so each spec needs a clean import rather than reusing a previous
 * singleton instance.
 *
 * @returns {Promise<import('@/store/useStore').default>} Fresh Zustand store instance.
 */
async function loadFreshStore() {
  vi.resetModules()
  const module = await import('@/store/useStore')
  return module.default
}

/**
 * Seeds the legacy browser-storage keys that used to drive startup hydration.
 *
 * If any future refactor accidentally reintroduces storage-based hydration,
 * these values will leak into the store and immediately fail the startup
 * assertions below.
 */
function seedLegacyBrowserState() {
  const legacyConfig = {
    ...DEFAULT_CONFIG,
    scene: {
      ...DEFAULT_CONFIG.scene,
      start: 12,
      end: 144,
      font: 'LegacyFont.ttf',
    },
  }

  localStorage.setItem('previewInterpolationEnabled', 'false')
  localStorage.setItem('dummyDurationSeconds', '144')
  localStorage.setItem('startSecond', '12')
  localStorage.setItem('endSecond', '144')
  localStorage.setItem('selectedSecond', '12')
  localStorage.setItem('autoRender', 'true')
  localStorage.setItem('editorConfig', JSON.stringify(legacyConfig))
  localStorage.setItem('updateRate', '5')
  localStorage.setItem('exportCodec', 'libx264')
  localStorage.setItem('aspectRatio', '9:16')
  localStorage.setItem('globalDefaults', JSON.stringify({ color_text: '#ff00ff' }))
  localStorage.setItem('loadedTemplateFilename', 'legacy-template.json')
  localStorage.setItem('loadedTemplateSource', 'file')
  localStorage.setItem(
    'lastSavedTemplateState',
    JSON.stringify({
      config: legacyConfig,
      settings: {
        globalDefaults: {
          color_text: '#ff00ff',
        },
      },
    }),
  )
  localStorage.setItem('videoFilename', 'legacy-video.mp4')
}

describe('useStore startup', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  afterEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
  })

  test('uses in-memory defaults instead of restoring legacy browser storage', async () => {
    // Simulate a user reopening the app after an older build persisted state.
    seedLegacyBrowserState()

    const useStore = await loadFreshStore()
    const state = useStore.getState()

    expect(state.previewInterpolationEnabled).toBe(true)
    expect(state.dummyDurationSeconds).toBe(73)
    expect(state.startSecond).toBe(0)
    expect(state.endSecond).toBe(73)
    expect(state.selectedSecond).toBe(0)
    expect(state.autoRender).toBe(false)
    expect(state.config).toEqual(DEFAULT_CONFIG)
    expect(state.updateRate).toBe(1)
    expect(state.exportRange).toEqual(DEFAULT_EXPORT_RANGE)
    expect(state.exportCodec).toBe('prores_ks')
    expect(state.globalDefaults).toEqual(DEFAULT_GLOBAL_DEFAULTS)
    expect(state.aspectRatio).toBe('16:9')
    expect(state.loadedTemplateFilename).toBeNull()
    expect(state.loadedTemplateSource).toBeNull()
    expect(state.lastSavedTemplateState).toBeNull()
    expect(state.videoFilename).toBeNull()
  })

  test('module evaluation does not touch browser storage', async () => {
    // Import-time purity matters because these modules must stay safe in tests,
    // node contexts, and any future non-browser startup path.
    vi.resetModules()
    const getItemSpy = vi.spyOn(Storage.prototype, 'getItem')
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem')
    const removeItemSpy = vi.spyOn(Storage.prototype, 'removeItem')

    await import('@/store/useStore')
    await import('@/features/app-shell/hooks/useEditorShellState')

    expect(getItemSpy).not.toHaveBeenCalled()
    expect(setItemSpy).not.toHaveBeenCalled()
    expect(removeItemSpy).not.toHaveBeenCalled()
  })

  test('explicit template loading updates durable active state without browser storage persistence', async () => {
    // Template loading is the allowed initialization path now, but it must
    // update live state directly instead of rebuilding persistence.
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem')
    const removeItemSpy = vi.spyOn(Storage.prototype, 'removeItem')
    const useStore = await loadFreshStore()
    const templateConfig = {
      ...DEFAULT_CONFIG,
      scene: {
        ...DEFAULT_CONFIG.scene,
        start: 5,
        end: 90,
        font: 'TemplateFont.ttf',
      },
    }

    useStore.getState().hydrateTemplateState(
      {
        config: templateConfig,
        settings: {
          globalDefaults: {
            color_text: '#123456',
          },
        },
      },
      {
        filename: 'imported-template.json',
        source: 'file',
      },
    )

    const state = useStore.getState()
    const effectiveConfig = createEditorEffectiveConfig({
      config: state.config,
      globalDefaults: state.globalDefaults,
    })

    expect(state.config.scene.start).toBe(5)
    expect(state.config.scene.end).toBe(90)
    expect(state.config.scene).not.toHaveProperty('font')
    expect(state.config.scene).not.toHaveProperty('color')
    expect(state.config.scene).not.toHaveProperty('font_size')
    expect(effectiveConfig.scene.font).toBe('Arial.ttf')
    expect(effectiveConfig.scene.color).toBe('#123456')
    expect(state.startSecond).toBe(5)
    expect(state.endSecond).toBe(90)
    expect(state.selectedSecond).toBe(5)
    expect(state.loadedTemplateFilename).toBe('imported-template.json')
    expect(state.loadedTemplateSource).toBe('file')
    expect(state.globalDefaults.color_text).toBe('#123456')
    expect(setItemSpy).not.toHaveBeenCalled()
    expect(removeItemSpy).not.toHaveBeenCalled()
  })

  test('restarting after runtime edits returns to clean defaults until a template is loaded again', async () => {
    const useStore = await loadFreshStore()
    const templateConfig = {
      ...DEFAULT_CONFIG,
      scene: {
        ...DEFAULT_CONFIG.scene,
        start: 8,
        end: 101,
        font: 'SessionFont.ttf',
      },
    }

    useStore.getState().hydrateTemplateState(
      {
        config: templateConfig,
        settings: {
          globalDefaults: {
            color_text: '#abcdef',
          },
        },
      },
      {
        filename: 'session-template.json',
        source: 'file',
      },
    )
    useStore.getState().setPreviewInterpolationEnabled(false)
    useStore.getState().setAutoRender(true)
    useStore.getState().setUpdateRate(5)
    useStore.getState().setExportCodec('libx264')
    useStore.getState().setAspectRatio('9:16')
    useStore.getState().setVideoFilename('session-video.mp4')

    const restartedStore = await loadFreshStore()
    const restartedState = restartedStore.getState()

    expect(restartedState.previewInterpolationEnabled).toBe(true)
    expect(restartedState.autoRender).toBe(false)
    expect(restartedState.config).toEqual(DEFAULT_CONFIG)
    expect(restartedState.updateRate).toBe(1)
    expect(restartedState.exportRange).toEqual(DEFAULT_EXPORT_RANGE)
    expect(restartedState.exportCodec).toBe('prores_ks')
    expect(restartedState.globalDefaults).toEqual(DEFAULT_GLOBAL_DEFAULTS)
    expect(restartedState.aspectRatio).toBe('16:9')
    expect(restartedState.loadedTemplateFilename).toBeNull()
    expect(restartedState.loadedTemplateSource).toBeNull()
    expect(restartedState.videoFilename).toBeNull()
  })
})
