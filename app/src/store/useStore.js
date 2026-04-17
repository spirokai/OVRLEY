import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'
import { devtools } from 'zustand/middleware'
import * as backend from '../api/backend'

// Flags to prevent circular updates
let isUpdatingFromConfig = false
let isUpdatingFromTimeline = false

const DEFAULT_CONFIG = {
  scene: {
    width: 1920,
    height: 1080,
    fps: 30,
    start: 0,
    end: 60,
    font: 'Arial.ttf',
    color: '#ffffff',
    font_size: 30,
  },
  labels: [],
  values: [],
  plots: [],
}

const useStore = create(
  devtools(
    immer((set, get) => ({
      communityTemplateFilename: null,
      loadedTemplateFilename:
        localStorage.getItem('loadedTemplateFilename') || null,
      templates: [],
      editor: null,
      generatingImage: false,
      renderingVideo: false,
      errorMessage: null, // For displaying user-friendly errors
      hasUnrenderedChanges: false,
      lastRenderedConfig: null,
      updateRate: 1, // 1/1, 1/2, 1/4, 1/8
      exportRange: {
        type: 'all', // 'all' or 'custom'
        from: 0,
        to: 0,
        fromTime: '00:00:00',
        toTime: '00:00:00',
      },
      globalDefaults: {
        font_values: 'Arial.ttf',
        font_text: 'Arial.ttf',
        color_values: '#ffffff',
        color_text: '#ffffff',
        color_icons: '#ffffff',
        border_color: '#000000',
        border_thickness: 0,
        shadow_color: '#00000066',
        shadow_strength: 0,
        shadow_distance: 0,
        opacity: 1.0,
        scale: 1.0,
      },
      aspectRatio: localStorage.getItem('aspectRatio') || '16:9',
      autoRender: localStorage.getItem('autoRender') === 'true',
      imageFilename: localStorage.getItem('imageFilename') || null,
      videoFilename: localStorage.getItem('videoFilename') || null,
      gpxFilename: localStorage.getItem('gpxFilename') || null,
      renderProgress: {
        current: 0,
        total: 0,
        percent: 0,
        status: 'idle',
        message: '',
        estimatedSecondsRemaining: null,
      },

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

      // Slider states - load from localStorage if available
      dummyDurationSeconds: (() => {
        const saved = localStorage.getItem('dummyDurationSeconds')
        return saved ? parseInt(saved, 10) : 73
      })(),
      startSecond: (() => {
        const saved = localStorage.getItem('startSecond')
        return saved ? parseInt(saved, 10) : 0
      })(),
      endSecond: (() => {
        const saved = localStorage.getItem('endSecond')
        return saved ? parseInt(saved, 10) : 73
      })(),
      selectedSecond: (() => {
        const saved = localStorage.getItem('selectedSecond')
        return saved ? parseInt(saved, 10) : 0
      })(),

      config: (() => {
        const savedConfig = localStorage.getItem('editorConfig')
        if (savedConfig) {
          try {
            const parsed = JSON.parse(savedConfig)
            if (parsed && parsed.scene) return parsed
          } catch {
            console.warn('Failed to parse saved config, using default')
          }
        }
        return DEFAULT_CONFIG
      })(), // This will hold the editor config
      setConfig: (val) => {
        const currentState = get()

        // Check if new config differs from what was last rendered
        const isDifferent = currentState.lastRenderedConfig
          ? JSON.stringify(val) !==
            JSON.stringify(currentState.lastRenderedConfig)
          : false

        localStorage.setItem('editorConfig', JSON.stringify(val))

        // Set flag to prevent timeline setters from updating config
        const wasUpdating = isUpdatingFromConfig
        isUpdatingFromConfig = true

        set((state) => {
          state.config = val

          if (val.scene) {
            // Only update timeline if we don't have values yet (initial load)
            const hasExistingTimeline =
              state.startSecond !== 0 ||
              state.endSecond !== state.dummyDurationSeconds

            if (!hasExistingTimeline) {
              // Use config values for timeline
              if (val.scene.start !== undefined) {
                state.startSecond = val.scene.start
              }
              if (val.scene.end !== undefined) {
                state.endSecond = val.scene.end
                state.dummyDurationSeconds = val.scene.end
              }
              if (val.scene.start !== undefined) {
                state.selectedSecond = val.scene.start
              }
            } else {
              // User has edited start/end in the editor - update timeline to match
              if (
                val.scene.start !== undefined &&
                val.scene.start !== state.startSecond
              ) {
                state.startSecond = val.scene.start
                localStorage.setItem('startSecond', val.scene.start.toString())
              }
              if (
                val.scene.end !== undefined &&
                val.scene.end !== state.endSecond
              ) {
                state.endSecond = val.scene.end
                state.dummyDurationSeconds = val.scene.end
                localStorage.setItem('endSecond', val.scene.end.toString())
                localStorage.setItem(
                  'dummyDurationSeconds',
                  val.scene.end.toString(),
                )
              }
            }
          }

          // Mark that we have changes that haven't been rendered yet
          if (!wasUpdating) {
            state.hasUnrenderedChanges = isDifferent
          }
        })

        // Reset flag after a short delay
        setTimeout(() => {
          isUpdatingFromConfig = false
        }, 100)
      },

      setHasUnrenderedChanges: (val) =>
        set((state) => {
          state.hasUnrenderedChanges = val
        }),
      setLastRenderedConfig: (config) =>
        set((state) => {
          state.lastRenderedConfig = JSON.parse(JSON.stringify(config))
        }),

      setAutoRender: (val) => {
        localStorage.setItem('autoRender', val.toString())
        set((state) => {
          state.autoRender = val
        })
      },

      setGeneratingImage: (generating) =>
        set((state) => {
          state.generatingImage = generating
        }),
      setRenderingVideo: (rendering) =>
        set((state) => {
          state.renderingVideo = rendering
        }),
      setRenderProgress: (progress) => {
        const percent =
          progress.total > 0
            ? Math.round((progress.current / progress.total) * 100)
            : 0
        set((state) => {
          state.renderProgress = { ...progress, percent }
        })
      },
      lastSavedConfig: localStorage.getItem('lastSavedConfig')
        ? JSON.parse(localStorage.getItem('lastSavedConfig'))
        : null,
      setLastSavedConfig: (config) => {
        localStorage.setItem('lastSavedConfig', JSON.stringify(config))
        set((state) => {
          state.lastSavedConfig = config
        })
      },
      setErrorMessage: (message) =>
        set((state) => {
          state.errorMessage = message
        }),
      clearError: () =>
        set((state) => {
          state.errorMessage = null
        }),

      setUpdateRate: (rate) =>
        set((state) => {
          state.updateRate = rate
        }),
      setExportRange: (range) =>
        set((state) => {
          state.exportRange = { ...state.exportRange, ...range }
        }),
      setGlobalDefault: (key, value) =>
        set((state) => {
          state.globalDefaults[key] = value
        }),
      setAspectRatio: (ratio) => {
        localStorage.setItem('aspectRatio', ratio)
        set((state) => {
          state.aspectRatio = ratio
        })
      },
      resetGlobalDefaults: () =>
        set((state) => {
          state.globalDefaults = {
            font_values: 'Arial.ttf',
            font_text: 'Arial.ttf',
            color_values: '#ffffff',
            color_text: '#ffffff',
            color_icons: '#ffffff',
            border_color: '#000000',
            border_thickness: 0,
            border_strength: 0,
            border_distance: 0,
            shadow_color: '#00000066',
            shadow_strength: 0,
            shadow_distance: 0,
            opacity: 1.0,
            scale: 1.0,
          }
        }),

      setDummyDurationSeconds: (duration) => {
        localStorage.setItem('dummyDurationSeconds', duration.toString())
        set((state) => {
          state.dummyDurationSeconds = duration
        })
      },

      setStartSecond: (second) => {
        localStorage.setItem('startSecond', second.toString())

        const state = get()
        if (state.startSecond === second) return // No change

        set((state) => {
          state.startSecond = second

          // Always update config to match, unless we're in the middle of loading config
          if (!isUpdatingFromConfig && state.config && state.config.scene) {
            if (state.config.scene.start === second) return

            state.config.scene.start = second

            // Check against last rendered
            if (state.lastRenderedConfig) {
              state.hasUnrenderedChanges =
                JSON.stringify(state.config) !==
                JSON.stringify(state.lastRenderedConfig)
            } else {
              state.hasUnrenderedChanges = true
            }

            localStorage.setItem('editorConfig', JSON.stringify(state.config))
          }
        })
      },

      setEndSecond: (second) => {
        localStorage.setItem('endSecond', second.toString())

        const state = get()
        if (state.endSecond === second) return // No change

        set((state) => {
          state.endSecond = second

          // Always update config to match, unless we're in the middle of loading config
          if (!isUpdatingFromConfig && state.config && state.config.scene) {
            if (state.config.scene.end === second) return

            state.config.scene.end = second

            // Check against last rendered
            if (state.lastRenderedConfig) {
              state.hasUnrenderedChanges =
                JSON.stringify(state.config) !==
                JSON.stringify(state.lastRenderedConfig)
            } else {
              state.hasUnrenderedChanges = true
            }

            localStorage.setItem('editorConfig', JSON.stringify(state.config))
          }
        })
      },

      setSelectedSecond: (second) => {
        localStorage.setItem('selectedSecond', second.toString())
        set((state) => {
          state.selectedSecond = second
        })
      },

      setImageFilename: (filename) => {
        localStorage.setItem('imageFilename', filename)
        set((state) => {
          state.imageFilename = filename
        })
      },

      setVideoFilename: (filename) => {
        localStorage.setItem('videoFilename', filename)
        set((state) => {
          state.videoFilename = filename
        })
      },

      setGpxFilename: async (filename) => {
        localStorage.setItem('gpxFilename', filename)
        set((state) => {
          state.gpxFilename = filename
        })

        const isLikelyGpx =
          typeof filename === 'string' &&
          (filename.endsWith('.gpx') || filename.startsWith('http'))
        if (!isLikelyGpx) return

        try {
          const response = await fetch(filename)
          const fileBlob = await response.blob()
          const reader = new FileReader()
          reader.onloadend = () => {}
          reader.readAsDataURL(fileBlob)
        } catch {
          console.warn(
            'setGpxFilename: could not fetch GPX file contents (expected for demo)',
          )
        }
      },

      setGpxFilenameFromFile: (file) => {
        set((state) => {
          state.gpxFilename = file ? file['name'] : null
        })
      },

      setLoadedTemplateFilename: (filename) => {
        localStorage.setItem('loadedTemplateFilename', filename || '')
        set((state) => {
          state.communityTemplateFilename = null
          state.loadedTemplateFilename = filename
        })
      },

      setEditor: (editor) =>
        set((state) => {
          state.editor = editor
        }),

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

          // If no GPX file is loaded, automatically load the demo
          if (!state.gpxFilename) {
            get().setGpxFilename('demo.gpxinit')
            const demoDuration = 7946 // seward.gpx duration
            get().setDummyDurationSeconds(demoDuration)
            get().setStartSecond(0)
            get().setEndSecond(demoDuration)
            get().setSelectedSecond(0)
          }

          // Always update the config in the store
          get().setConfig(data)

          if (state.editor) {
            state.editor.setValue(data)
          } else {
            const { default: generateDemoFrame } =
              await import('../api/generateDemoFrame.jsx')
            await generateDemoFrame(data)
          }
        } catch (error) {
          console.error('Error with community templates:', error)
          alert(`Failed to load template: ${error.message}`)
        }
      },
    })),
    {
      name: 'CyclemetryStore',
      serialize: {
        replacer: (key, value) =>
          key === 'editor' ? '<<MonacoEditor>>' : value,
      },
    },
  ),
)

// Export function to check if we're updating from timeline
export const isUpdatingFromTimelineFlag = () => isUpdatingFromTimeline

export default useStore
