/**
 * Characterization tests for createTemplateSlice store actions.
 *
 * These tests verify that store actions are pure state transitions and do not
 * perform network I/O, browser UI work, or imperative editor manipulation.
 * They freeze the current state-only actions and serve as regression guards
 * for the side-effect extraction refactoring.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template-state'
import { createEditorEffectiveConfig } from '@/lib/template-state'
import { DEFAULT_CONFIG } from '@/store/store-utils'

/**
 * Re-imports the store after resetting the module graph so each test gets a
 * fresh store instance.
 *
 * @returns {Promise<import('@/store/useStore').default>} Fresh Zustand store instance.
 */
async function loadFreshStore() {
  vi.resetModules()
  const module = await import('@/store/useStore')
  return module.default
}

describe('createTemplateSlice — pure state actions', () => {
  let useStore

  beforeEach(async () => {
    useStore = await loadFreshStore()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  test('setTemplates is pure — updates state without network or UI side effects', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const alertSpy = vi.spyOn(window, 'alert').mockImplementation(() => {})
    const templateList = [{ name: 'test-template.json', source: 'backend' }]

    useStore.getState().setTemplates(templateList)

    expect(useStore.getState().templates).toEqual(templateList)
    expect(fetchSpy).not.toHaveBeenCalled()
    expect(alertSpy).not.toHaveBeenCalled()
  })

  test('setLastSavedTemplateState is pure — updates lastSavedTemplateState without network', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const templateState = { config: {}, settings: { globalDefaults: {} } }

    useStore.getState().setLastSavedTemplateState(templateState)

    expect(useStore.getState().lastSavedTemplateState).toEqual(templateState)
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  test('setUpdateRate is pure — updates updateRate in state', () => {
    useStore.getState().setUpdateRate(5)

    expect(useStore.getState().updateRate).toBe(5)
  })

  test('setExportRange is pure — merges into exportRange', () => {
    useStore.getState().setExportRange({ fromTime: '00:00:05', toTime: '00:01:00' })

    expect(useStore.getState().exportRange.type).toBe(DEFAULT_EXPORT_RANGE.type)
    expect(useStore.getState().exportRange.fromTime).toBe('00:00:05')
    expect(useStore.getState().exportRange.toTime).toBe('00:01:00')
  })

  test('setExportCodec is pure — updates exportCodec and normalizes', () => {
    useStore.getState().setPlatformOs('macos')
    useStore.getState().setExportCodec('prores_videotoolbox')

    expect(useStore.getState().exportCodec).toBe('prores_videotoolbox')
  })

  test('setPlatformOs is pure — updates platformOs and normalizes codec', () => {
    useStore.getState().setPlatformOs('macos')

    expect(useStore.getState().platformOs).toBe('macos')
  })

  test('setGlobalDefault is pure — updates globalDefaults and keeps widget config synchronized for editing', () => {
    useStore.getState().setConfig({
      ...DEFAULT_CONFIG,
      values: [
        {
          id: 'value-1',
          value: 'speed',
          x: 10,
          y: 20,
          icon_color: '#111111',
          unit_color: '#222222',
        },
      ],
    })

    useStore.getState().setGlobalDefault('color_values', '#ff0000')
    useStore.getState().setGlobalDefault('color_icons', '#00ff00')
    useStore.getState().setGlobalDefault('color_units', '#0000ff')

    const effectiveConfig = createEditorEffectiveConfig({
      config: useStore.getState().config,
      globalDefaults: useStore.getState().globalDefaults,
    })

    useStore.getState().setGlobalDefault('color_text', '#ff0000')

    expect(useStore.getState().globalDefaults.color_text).toBe('#ff0000')
    expect(useStore.getState().config.values[0].color).toBe('#ff0000')
    expect(useStore.getState().config.values[0].icon_color).toBe('#00ff00')
    expect(useStore.getState().config.values[0].unit_color).toBe('#0000ff')
    expect(effectiveConfig.values[0].color).toBe('#ff0000')
  })

  test('setAspectRatio is pure — updates aspectRatio', () => {
    useStore.getState().setAspectRatio('21:9')

    expect(useStore.getState().aspectRatio).toBe('21:9')
  })

  test('createNewTemplate is pure — resets state to defaults without network', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    useStore.getState().setUpdateRate(5)
    useStore.getState().createNewTemplate()

    expect(useStore.getState().config).toEqual(DEFAULT_CONFIG)
    expect(useStore.getState().globalDefaults).toEqual(DEFAULT_GLOBAL_DEFAULTS)
    expect(useStore.getState().updateRate).toBe(1)
    expect(useStore.getState().loadedTemplateFilename).toBeNull()
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  test('resetGlobalDefaults is pure — resets globals and syncs config', () => {
    useStore.getState().setGlobalDefault('color_text', '#ff0000')
    useStore.getState().resetGlobalDefaults()

    expect(useStore.getState().globalDefaults).toEqual(DEFAULT_GLOBAL_DEFAULTS)
  })

  test('setLoadedTemplate is pure — sets filename and source', () => {
    useStore.getState().setLoadedTemplate('my-template.json', 'file')

    expect(useStore.getState().loadedTemplateFilename).toBe('my-template.json')
    expect(useStore.getState().loadedTemplateSource).toBe('file')
  })

  test('setCommunityTemplateFilename is a pure setter — no network I/O or UI side effects', () => {
    const state = useStore.getState()
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const alertSpy = vi.spyOn(window, 'alert').mockImplementation(() => {})

    expect(state.SelectCommunityTemplateFilename).toBeUndefined()
    expect(state.setCommunityTemplateFilename).toBeTypeOf('function')

    state.setCommunityTemplateFilename('demo-template.json')

    expect(useStore.getState().communityTemplateFilename).toBe('demo-template.json')
    expect(fetchSpy).not.toHaveBeenCalled()
    expect(alertSpy).not.toHaveBeenCalled()
  })

  test('hydrateTemplateState is pure — hydrates durable template state and keeps editor-effective values materializable', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const templateConfig = {
      ...DEFAULT_CONFIG,
      scene: { ...DEFAULT_CONFIG.scene, start: 10, end: 120 },
    }

    useStore.getState().hydrateTemplateState(
      {
        config: templateConfig,
        settings: { globalDefaults: { color_text: '#abcdef' } },
      },
      { filename: 'imported.json', source: 'file' },
    )

    const state = useStore.getState()
    const effectiveConfig = createEditorEffectiveConfig({
      config: state.config,
      globalDefaults: state.globalDefaults,
    })

    expect(state.config.scene.start).toBe(10)
    expect(state.config.scene.end).toBe(120)
    expect(state.config.scene).not.toHaveProperty('font')
    expect(state.config.scene).not.toHaveProperty('color')
    expect(state.config.scene).not.toHaveProperty('font_size')
    expect(effectiveConfig.scene.font).toBe('Arial.ttf')
    expect(effectiveConfig.scene.color).toBe('#abcdef')
    expect(state.startSecond).toBe(10)
    expect(state.endSecond).toBe(120)
    expect(state.loadedTemplateFilename).toBe('imported.json')
    expect(state.loadedTemplateSource).toBe('file')
    expect(fetchSpy).not.toHaveBeenCalled()
  })
})
