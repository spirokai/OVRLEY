import { DEFAULT_EXPORT_RANGE } from '@/lib/template/template-constants'

export const DEFAULT_RENDER_SETTINGS = Object.freeze({
  fps: 30,
  widgetUpdateRate: 1,
  exportMode: 'composite',
  codec: 'prores_ks',
  bitrateMbps: null,
  range: Object.freeze({ ...DEFAULT_EXPORT_RANGE }),
})

function validateRenderSettings(settings) {
  if (!Number.isFinite(settings.fps) || settings.fps <= 0) throw new Error('Render FPS must be a positive finite number')
  if (!Number.isInteger(settings.widgetUpdateRate) || settings.widgetUpdateRate <= 0) {
    throw new Error('Widget update rate must be a positive integer')
  }
  if (settings.exportMode !== 'transparent' && settings.exportMode !== 'composite') throw new Error('Invalid export mode')
  if (typeof settings.codec !== 'string' || !settings.codec) throw new Error('Render codec is required')
  if (settings.bitrateMbps !== null && (!Number.isFinite(settings.bitrateMbps) || settings.bitrateMbps <= 0)) {
    throw new Error('Render bitrate must be a positive finite number or null')
  }
  if (!settings.range || !['all', 'custom'].includes(settings.range.type)) throw new Error('Invalid export range type')
  if (!Number.isFinite(settings.range.from) || !Number.isFinite(settings.range.to)) throw new Error('Export range bounds must be finite')
  if (settings.range.type === 'custom' && settings.range.from >= settings.range.to) throw new Error('Custom export range requires from < to')
}

export function createRenderSettingsSlice(set) {
  return {
    renderSettings: {
      ...DEFAULT_RENDER_SETTINGS,
      range: { ...DEFAULT_RENDER_SETTINGS.range },
    },
    platformOs: 'unknown',

    setRenderSettings: (settings) => {
      validateRenderSettings(settings)
      set((state) => {
        state.renderSettings = { ...settings, range: { ...settings.range } }
      })
    },

    setRenderRange: (range) =>
      set((state) => {
        const nextRange = { ...state.renderSettings.range, ...range }
        validateRenderSettings({ ...state.renderSettings, range: nextRange })
        state.renderSettings.range = nextRange
      }),

    setRenderFpsAndUpdateRate: (fps, widgetUpdateRate) =>
      set((state) => {
        const next = { ...state.renderSettings, fps, widgetUpdateRate }
        validateRenderSettings(next)
        state.renderSettings.fps = fps
        state.renderSettings.widgetUpdateRate = widgetUpdateRate
      }),

    setRenderWidgetUpdateRate: (widgetUpdateRate) =>
      set((state) => {
        const next = { ...state.renderSettings, widgetUpdateRate }
        validateRenderSettings(next)
        state.renderSettings.widgetUpdateRate = widgetUpdateRate
      }),

    setPlatformOs: (platformOs) =>
      set((state) => {
        state.platformOs = platformOs || 'unknown'
      }),
  }
}
