import * as backend from '../../api/backend'
import { syncGlobalDefaultsToConfig } from '../../lib/config-utils'
import {
  DEFAULT_CONFIG,
  DEFAULT_EXPORT_RANGE,
  DEFAULT_GLOBAL_DEFAULTS,
  persistSerializable,
  readStoredJson,
  readStoredTemplateSettings,
  updateConfigPersistence,
} from '../store-utils'

const {
  updateRate: initialUpdateRate,
  exportRange: initialExportRange,
  exportCodec: initialExportCodec,
  globalDefaults: initialGlobalDefaults,
  aspectRatio: initialAspectRatio,
} = readStoredTemplateSettings()

export function createTemplateSlice(set, get) {
  return {
    communityTemplateFilename: null,
    loadedTemplateFilename:
      localStorage.getItem('loadedTemplateFilename') || null,
    loadedTemplateSource: localStorage.getItem('loadedTemplateSource') || null,
    templates: [],
    updateRate: initialUpdateRate,
    exportRange: initialExportRange,
    exportCodec: initialExportCodec,
    globalDefaults: initialGlobalDefaults,
    aspectRatio: initialAspectRatio,
    lastSavedTemplateState: readStoredJson('lastSavedTemplateState', null),

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

    setLastSavedTemplateState: (templateState) => {
      persistSerializable('lastSavedTemplateState', templateState)
      set((state) => {
        state.lastSavedTemplateState = templateState
      })
    },

    setUpdateRate: (rate) => {
      localStorage.setItem('updateRate', rate.toString())
      set((state) => {
        state.updateRate = rate
      })
    },

    setExportRange: (range) =>
      set((state) => {
        state.exportRange = { ...state.exportRange, ...range }
        persistSerializable('exportRange', state.exportRange)
      }),

    setExportCodec: (codec) => {
      localStorage.setItem('exportCodec', codec)
      set((state) => {
        state.exportCodec = codec
      })
    },

    setGlobalDefault: (key, value) => {
      const nextDefaults = { ...get().globalDefaults, [key]: value }

      set((state) => {
        state.globalDefaults = nextDefaults

        if (state.config) {
          state.config = syncGlobalDefaultsToConfig(
            state.config,
            nextDefaults,
            [key],
          )
          updateConfigPersistence(state)
        }

        persistSerializable('globalDefaults', state.globalDefaults)
      })
    },

    setAspectRatio: (ratio) => {
      localStorage.setItem('aspectRatio', ratio)
      set((state) => {
        state.aspectRatio = ratio
      })
    },

    resetGlobalDefaults: () => {
      set((state) => {
        state.globalDefaults = { ...DEFAULT_GLOBAL_DEFAULTS }

        if (state.config) {
          state.config = syncGlobalDefaultsToConfig(
            state.config,
            state.globalDefaults,
          )
          updateConfigPersistence(state)
        }

        persistSerializable('globalDefaults', state.globalDefaults)
      })
    },

    setLoadedTemplate: (filename, source = null) => {
      localStorage.setItem('loadedTemplateFilename', filename || '')
      localStorage.setItem('loadedTemplateSource', source || '')
      set((state) => {
        state.communityTemplateFilename = null
        state.loadedTemplateFilename = filename
        state.loadedTemplateSource = source
      })
    },

    hydrateTemplateState: (templateState, options = {}) => {
      const { filename = null, source = null } = options
      const nextConfig = templateState?.config || DEFAULT_CONFIG
      const nextSettings = templateState?.settings || {}
      const nextGlobalDefaults = {
        ...DEFAULT_GLOBAL_DEFAULTS,
        ...(nextSettings.globalDefaults || {}),
      }
      const nextExportRange = {
        ...DEFAULT_EXPORT_RANGE,
        ...(nextSettings.exportRange || {}),
      }
      const nextExportCodec = nextSettings.exportCodec || 'prores_ks'
      const nextAspectRatio = nextSettings.aspectRatio || '16:9'
      const nextUpdateRate = nextSettings.updateRate || 1

      localStorage.setItem('editorConfig', JSON.stringify(nextConfig))
      persistSerializable('globalDefaults', nextGlobalDefaults)
      persistSerializable('exportRange', nextExportRange)
      localStorage.setItem('exportCodec', nextExportCodec)
      localStorage.setItem('aspectRatio', nextAspectRatio)
      localStorage.setItem('updateRate', nextUpdateRate.toString())
      localStorage.setItem('loadedTemplateFilename', filename || '')
      localStorage.setItem('loadedTemplateSource', source || '')

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

        if (nextConfig.scene) {
          if (nextConfig.scene.start !== undefined) {
            state.startSecond = nextConfig.scene.start
            state.selectedSecond = nextConfig.scene.start
            localStorage.setItem(
              'startSecond',
              nextConfig.scene.start.toString(),
            )
            localStorage.setItem(
              'selectedSecond',
              nextConfig.scene.start.toString(),
            )
          }

          if (nextConfig.scene.end !== undefined) {
            state.endSecond = nextConfig.scene.end
            state.dummyDurationSeconds = nextConfig.scene.end
            localStorage.setItem('endSecond', nextConfig.scene.end.toString())
            localStorage.setItem(
              'dummyDurationSeconds',
              nextConfig.scene.end.toString(),
            )
          }
        }

        if (state.lastRenderedConfig) {
          state.hasUnrenderedChanges =
            JSON.stringify(nextConfig) !==
            JSON.stringify(state.lastRenderedConfig)
        } else {
          state.hasUnrenderedChanges = true
        }
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
        } else {
          const { default: generateDemoFrame } =
            await import('../../api/generateDemoFrame.jsx')
          await generateDemoFrame(data)
        }
      } catch (error) {
        console.error('Error with community templates:', error)
        alert(`Failed to load template: ${error.message}`)
      }
    },
  }
}
