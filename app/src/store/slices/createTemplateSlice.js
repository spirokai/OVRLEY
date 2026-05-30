/**
 * Creates the create template slice Zustand slice used by the application store.
 *
 * Store actions in this slice are pure state transitions only — no network I/O,
 * browser UI primitives, or imperative editor manipulation. Orchestration
 * concerns such as fetching template lists or loading community templates live
 * in dedicated hooks under features/template-manager/hooks/.
 */

import { normalizeColorFields, isColorFieldKey } from '../../lib/color-utils'
import { DEFAULT_EXPORT_RANGE } from '../../features/template-manager/data/templateConstants'
import { createDurableTemplateState, DEFAULT_GLOBAL_DEFAULTS, normalizeGlobalDefaults, syncGlobalDefaultsToConfig } from '../../lib/template-state'
import { cloneSerializable, DEFAULT_CONFIG, syncSceneTimingFromConfig, updateConfigPersistence } from '../store-utils'

const initialUpdateRate = 1
const initialExportRange = { ...DEFAULT_EXPORT_RANGE }
const initialExportCodec = 'prores_ks'
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
  const normalizePlatformCodec = (codec, platformOs) => {
    if (codec === 'libvpx-vp9' || codec === 'hevc_alpha') {
      return 'prores_ks'
    }

    if (codec === 'prores_videotoolbox' && platformOs !== 'macos') {
      return 'prores_ks'
    }

    return codec || 'prores_ks'
  }

  return {
    communityTemplateFilename: null,
    loadedTemplateFilename: null,
    loadedTemplateSource: null,
    templates: [],
    updateRate: initialUpdateRate,
    exportRange: initialExportRange,
    exportCodec: normalizePlatformCodec(initialExportCodec, 'unknown'),
    platformOs: 'unknown',
    globalDefaults: initialGlobalDefaults,
    aspectRatio: initialAspectRatio,
    lastSavedTemplateState: null,

    setTemplates: (templates) =>
      set((state) => {
        state.templates = templates
      }),

    setCommunityTemplateFilename: (filename) =>
      set((state) => {
        state.communityTemplateFilename = filename
      }),

    setLastSavedTemplateState: (templateState) =>
      set((state) => {
        state.lastSavedTemplateState = templateState
      }),

    setUpdateRate: (rate) =>
      set((state) => {
        state.updateRate = rate
      }),

    setExportRange: (range) =>
      set((state) => {
        state.exportRange = { ...state.exportRange, ...range }
      }),

    setExportCodec: (codec) => {
      const nextCodec = normalizePlatformCodec(codec, get().platformOs)
      set((state) => {
        state.exportCodec = nextCodec
      })
    },

    setPlatformOs: (platformOs) => {
      const nextPlatformOs = platformOs || 'unknown'
      set((state) => {
        state.platformOs = nextPlatformOs
        state.exportCodec = normalizePlatformCodec(state.exportCodec, nextPlatformOs)
      })
    },

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

    setAspectRatio: (ratio) =>
      set((state) => {
        state.aspectRatio = ratio
      }),

    createNewTemplate: () => {
      const nextConfig = cloneSerializable(DEFAULT_CONFIG)
      const nextGlobalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }
      const nextExportRange = { ...DEFAULT_EXPORT_RANGE }
      const nextExportCodec = 'prores_ks'
      const nextAspectRatio = '16:9'
      const nextUpdateRate = 1

      set((state) => {
        state.communityTemplateFilename = null
        state.config = nextConfig
        state.globalDefaults = nextGlobalDefaults
        state.exportRange = nextExportRange
        state.exportCodec = nextExportCodec
        state.aspectRatio = nextAspectRatio
        state.updateRate = nextUpdateRate
        state.loadedTemplateFilename = null
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

    setLoadedTemplate: (filename, source = null) =>
      set((state) => {
        state.communityTemplateFilename = null
        state.loadedTemplateFilename = filename
        state.loadedTemplateSource = source
      }),

    hydrateTemplateState: (templateState, options = {}) => {
      const { filename = null, source = null } = options
      const durableTemplateState = createDurableTemplateState({
        config: templateState?.config || DEFAULT_CONFIG,
        globalDefaults: templateState?.settings?.globalDefaults,
      })
      const nextConfig = durableTemplateState.config
      const nextGlobalDefaults = durableTemplateState.settings.globalDefaults
      const nextExportRange = {
        ...DEFAULT_EXPORT_RANGE,
        ...(get().exportRange || {}),
      }
      const nextExportCodec = normalizePlatformCodec(get().exportCodec || 'prores_ks', get().platformOs)
      const nextAspectRatio = get().aspectRatio || '16:9'
      const nextUpdateRate = get().updateRate || 1

      set((state) => {
        state.communityTemplateFilename = null
        state.config = nextConfig
        state.globalDefaults = nextGlobalDefaults
        state.exportRange = nextExportRange
        state.exportCodec = nextExportCodec
        state.aspectRatio = nextAspectRatio
        state.updateRate = nextUpdateRate
        state.loadedTemplateFilename = filename
        state.loadedTemplateSource = source

        syncSceneTimingFromConfig(state, nextConfig, { resetSelectedSecond: true })
        updateConfigPersistence(state)
      })
    },
  }
}
