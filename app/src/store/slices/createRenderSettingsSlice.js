import { QUALITY_DEFAULTS } from '@/features/render-video/data/qualityDefaults'
import { validateRenderQuality } from '@/features/render-video/utils/codecUtils'

export const DEFAULT_RENDER_SETTINGS = Object.freeze({
  outputMode: 'combined',
  fps: 30,
  widgetUpdateRate: 1,
  exportMode: 'composite',
  codec: 'prores_ks',
  qualityType: 'quality',
  qualityValue: QUALITY_DEFAULTS.h264,
})

/** @param {object} settings Canonical settings at store or submission ingress. @returns {void} */
export function validateRenderSettings(settings) {
  validateOutputMode(settings.outputMode)
  if (!Number.isFinite(settings.fps) || settings.fps <= 0) throw new Error('Render FPS must be a positive finite number')
  if (!Number.isInteger(settings.widgetUpdateRate) || settings.widgetUpdateRate <= 0) {
    throw new Error('Widget update rate must be a positive integer')
  }
  if (settings.exportMode !== 'transparent' && settings.exportMode !== 'composite') throw new Error('Invalid export mode')
  if (typeof settings.codec !== 'string' || !settings.codec) throw new Error('Render codec is required')
  validateRenderQuality(settings)
}

function validateOutputMode(outputMode) {
  if (outputMode !== 'combined' && outputMode !== 'separate') throw new Error('Invalid output mode')
}

export function createRenderSettingsSlice(set) {
  return {
    renderSettings: { ...DEFAULT_RENDER_SETTINGS },
    platformOs: 'unknown',

    setOutputMode: (outputMode) => {
      validateOutputMode(outputMode)
      set((state) => {
        state.renderSettings.outputMode = outputMode
      })
    },

    setRenderSettings: (settings) => {
      validateRenderSettings(settings)
      set((state) => {
        Object.assign(state.renderSettings, settings)
      })
    },

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
