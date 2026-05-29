/**
 * Creates the create template slice Zustand slice used by the application store.
 */

import * as backend from '../../api/backend'
import { normalizeColorFields, isColorFieldKey } from '../../lib/color-utils'
import { DEFAULT_GLOBAL_DEFAULTS, syncGlobalDefaultsToConfig } from '../../lib/config-utils'
import { DEFAULT_EXPORT_RANGE } from '../../features/template-manager'
import { cloneSerializable, DEFAULT_CONFIG, updateConfigPersistence } from '../store-utils'

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

  const applySceneTimingToState = (state, scene) => {
    if (!scene) return

    if (scene.start !== undefined) {
      state.startSecond = scene.start
      state.selectedSecond = scene.start
    }

    if (scene.end !== undefined) {
      state.endSecond = scene.end
    }
  }

  const normalizeGlobalDefaultsForState = (globalDefaults) => {
    const normalizedDefaults = normalizeColorFields(globalDefaults || {})
    return Object.keys(DEFAULT_GLOBAL_DEFAULTS).reduce(
      (result, key) => ({
        ...result,
        [key]: normalizedDefaults[key] === undefined ? DEFAULT_GLOBAL_DEFAULTS[key] : normalizedDefaults[key],
      }),
      {},
    )
  }

  const updateUnrenderedChanges = (state, nextConfig) => {
    if (state.lastRenderedConfig) {
      state.hasUnrenderedChanges = JSON.stringify(nextConfig) !== JSON.stringify(state.lastRenderedConfig)
      return
    }

    state.hasUnrenderedChanges = true
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

    fetchTemplates: async () => {
      try {
        const templates = await backend.listTemplates()
        set((state) => {
          state.templates = templates
        })
      } catch (err) {
        console.error('Failed to fetch templates:', err)
      }
    },

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
      const nextDefaults = {
        ...get().globalDefaults,
        [key]: isColorFieldKey(key) ? normalizeColorFields({ [key]: value })[key] : value,
      }

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
        applySceneTimingToState(state, nextConfig.scene)
        updateUnrenderedChanges(state, nextConfig)
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
      const nextConfig = templateState?.config || DEFAULT_CONFIG
      const nextSettings = templateState?.settings || {}
      const nextGlobalDefaults = normalizeGlobalDefaultsForState(nextSettings.globalDefaults)
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

        applySceneTimingToState(state, nextConfig.scene)
        updateUnrenderedChanges(state, nextConfig)
      })
    },

    SelectCommunityTemplateFilename: async (filename) => {
      set((state) => {
        state.loadedTemplateFilename = null
        state.communityTemplateFilename = filename
      })

      if (!filename) return

      try {
        const url = `/templates/${filename}`
        const response = await fetch(url)

        if (!response.ok) {
          throw new Error(`Network response was not ok: ${response.status}`)
        }

        const data = await response.json()
        const state = get()

        if (!state.gpxFilename) {
          get().setGpxFilename('demo.gpxinit')
          const demoDuration = 7946
          get().setDummyDurationSeconds(demoDuration)
          get().setStartSecond(0)
          get().setEndSecond(demoDuration)
          get().setSelectedSecond(0)
        }

        get().setConfig(data)

        if (state.editor) {
          state.editor.setValue(data)
        }
      } catch (error) {
        console.error('Error with community templates:', error)
        alert(`Failed to load template: ${error.message}`)
      }
    },
  }
}
