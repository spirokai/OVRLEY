/**
 * Creates the create template slice Zustand slice used by the application store.
 *
 * Store actions in this slice are pure state transitions only — no network I/O,
 * browser UI primitives, or imperative editor manipulation. Orchestration
 * concerns such as fetching template lists or loading community templates live
 * in dedicated hooks under features/template-manager/hooks/.
 */

import { normalizeColorFields, isColorFieldKey } from '../../lib/color-utils'
import { DEFAULT_GLOBAL_DEFAULTS } from '../../lib/template/template-constants'
import { syncGlobalDefaultsToConfig } from '../../lib/template/template-state'
import { normalizeGlobalDefaults } from '../../lib/template/template-normalization'
import { cloneSerializable, DEFAULT_CONFIG, syncSceneTimingFromConfig, updateConfigPersistence } from '../store-utils'

const initialGlobalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }
const initialAspectRatio = '16:9'

/**
 * Creates template slice.
 *
 * @param {*} set - Zustand setter callback.
 * @param {*} get - Value for get.
 * @returns {object} Derived data structure for downstream use.
 */
export function createTemplateSlice(set, get) {
  return {
    loadedTemplateSource: null,
    templates: [],
    globalDefaults: initialGlobalDefaults,
    aspectRatio: initialAspectRatio,
    lastSavedTemplateState: null,

    setTemplates: (templates) =>
      set((state) => {
        state.templates = templates
      }),

    setLastSavedTemplateState: (templateState) =>
      set((state) => {
        state.lastSavedTemplateState = templateState
      }),

    setGlobalDefault: (key, value) => {
      const nextDefaults = normalizeGlobalDefaults({
        ...get().globalDefaults,
        [key]: isColorFieldKey(key) ? normalizeColorFields({ [key]: value })[key] : value,
      })

      set((state) => {
        state.globalDefaults = nextDefaults

        if (state.config) {
          state.config = syncGlobalDefaultsToConfig(state.config, nextDefaults, [key])
          updateConfigPersistence(state)
        }
      })
    },

    setCustomAspectRatio: () =>
      set((state) => {
        state.aspectRatio = 'custom'
      }),

    setAspectRatioPreset: (ratio, resolution) =>
      set((state) => {
        state.aspectRatio = ratio
        state.config.scene.width = resolution.width
        state.config.scene.height = resolution.height
        updateConfigPersistence(state)
      }),

    createNewTemplate: () => {
      const nextConfig = cloneSerializable(DEFAULT_CONFIG)
      const nextGlobalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }
      const nextAspectRatio = '16:9'

      set((state) => {
        state.config = nextConfig
        state.globalDefaults = nextGlobalDefaults
        state.aspectRatio = nextAspectRatio
        state.loadedTemplateSource = null
        state.lastSavedTemplateState = null
        syncSceneTimingFromConfig(state, nextConfig, { resetSelectedSecond: true })
        updateConfigPersistence(state)
      })
    },

    resetGlobalDefaults: () => {
      set((state) => {
        state.globalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }

        if (state.config) {
          state.config = syncGlobalDefaultsToConfig(state.config, state.globalDefaults)
          updateConfigPersistence(state)
        }
      })
    },

    setLoadedTemplateSource: (source) =>
      set((state) => {
        state.loadedTemplateSource = source
      }),

    hydrateTemplateState: (templateState, options = {}) => {
      const { source = null } = options
      const nextConfig = templateState.config
      const nextGlobalDefaults = templateState.settings.globalDefaults
      const nextAspectRatio = get().aspectRatio || '16:9'

      set((state) => {
        state.config = nextConfig
        state.globalDefaults = nextGlobalDefaults
        state.aspectRatio = nextAspectRatio
        state.loadedTemplateSource = source

        syncSceneTimingFromConfig(state, nextConfig, { resetSelectedSecond: true })
        updateConfigPersistence(state)
      })
    },
  }
}
