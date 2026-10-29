/**
 * @file useSceneSettingsState – Container hook for the scene settings sidebar tab.
 *
 * Orchestrates store access, local state, derived state, effects, and event
 * handlers. Returns grouped objects so consumers can pass them as coherent
 * blocks instead of manually destructuring many individual keys.
 *
 * Return shape:
 * - overlaySettings   scene resolution, FPS, update rate, aspect ratio, video mismatch
 * - globalSettings    global defaults, fonts, scene style accessor
 * - handlers          all event handlers (each section destructures what it needs)
 *
 * @module useSceneSettingsState
 */

import { useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import useStore from '@/store/useStore'
import { useAvailableFonts, useFontPreparation } from '@/hooks/useFonts'
import { getPreparedFont } from '@/lib/font-resources'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { getUpdateRateOptions, normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { RESOLUTIONS } from '../data/sceneSettingsConstants'
import { sanitizeNumber } from '../utils/sceneSettingsUtils'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { getWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { scaleWidgetPositions } from '@/lib/widget/widget-config'

function getResolutionPresetId(scene) {
  if (!scene) return '1080p'
  const match = Object.values(RESOLUTIONS)
    .flat()
    .find((resolution) => resolution.w === scene.width && resolution.h === scene.height)
  return match ? match.id : 'custom'
}

function getSceneResolutionKey(scene) {
  if (!scene) return null
  return `${Number(scene.width)}x${Number(scene.height)}`
}

function getResolutionPreset(aspectRatio, resolutionId) {
  const presets = RESOLUTIONS[aspectRatio]
  if (!presets) {
    throw new Error(`Unknown aspect ratio "${aspectRatio}"`)
  }

  for (const preset of presets) {
    if (preset.id === resolutionId) return preset
  }

  throw new Error(`Unknown resolution preset "${resolutionId}" for aspect ratio "${aspectRatio}"`)
}

function getDefaultResolutionPreset(aspectRatio) {
  const presets = RESOLUTIONS[aspectRatio]
  if (!presets) {
    throw new Error(`Unknown aspect ratio "${aspectRatio}"`)
  }
  return presets[0]
}

export default function useSceneSettingsState({ config, onConfigChange, widgetLiveEdits }) {
  const {
    activitySummary,
    aspectRatio,
    globalDefaults: committedGlobalDefaults,
    importedVideoFps,
    importedVideoResolution,
    setAspectRatioPreset,
    setCustomAspectRatio,
    renderSettings,
    setRenderRange,
    setGlobalDefault,
    setRenderFpsAndUpdateRate,
    setRenderWidgetUpdateRate,
  } = useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      aspectRatio: state.aspectRatio,
      globalDefaults: state.globalDefaults,
      importedVideoFps: state.importedVideoFps,
      importedVideoResolution: state.importedVideoResolution,
      setAspectRatioPreset: state.setAspectRatioPreset,
      setCustomAspectRatio: state.setCustomAspectRatio,
      renderSettings: state.renderSettings,
      setRenderRange: state.setRenderRange,
      setGlobalDefault: state.setGlobalDefault,
      setRenderFpsAndUpdateRate: state.setRenderFpsAndUpdateRate,
      setRenderWidgetUpdateRate: state.setRenderWidgetUpdateRate,
    })),
  )

  const { liveGlobalDefaults } = useWidgetDraftView(widgetLiveEdits)
  const globalDefaults = useMemo(() => ({ ...committedGlobalDefaults, ...liveGlobalDefaults }), [committedGlobalDefaults, liveGlobalDefaults])
  // Fixed point of a scale drag, captured from the committed layout when the drag starts.
  const scaleOriginRef = useRef(null)
  const availableFonts = useAvailableFonts()
  const prepareFontEdit = useFontPreparation()
  const getScaleOrigin = () => {
    const widgets = buildConfigWidgets(useStore.getState().config)
    if (!widgets.length) return null
    return getWidgetInteraction(widgetLiveEdits.getWidgetNode(widgets[0].id))
      .group(widgets.map((widget) => widget.id))
      .bounds()
  }
  const changeGlobalDefault = (key, value) => {
    if (key === 'font_text' || key === 'font_values') {
      prepareFontEdit(key, [value], () => {
        useStore.getState().setGlobalDefault(key, value, key === 'font_text' ? getPreparedFont(value) : undefined)
      })
    } else setGlobalDefault(key, value)
  }
  const draftGlobalDefault = (key, value) => {
    if (key === 'scale') {
      if (!Object.hasOwn(widgetLiveEdits.getGlobalDefaultDrafts(), 'scale')) scaleOriginRef.current = getScaleOrigin()
      const { config: committedConfig, globalDefaults: committed } = useStore.getState()
      const scaledConfig = scaleWidgetPositions(committedConfig, value / committed.scale, scaleOriginRef.current)
      widgetLiveEdits.setLiveWidgetDrafts(
        Object.fromEntries(buildConfigWidgets(scaledConfig).map((widget) => [widget.id, { x: widget.data.x, y: widget.data.y }])),
      )
    }
    widgetLiveEdits.setLiveGlobalDefault(key, value)
  }
  const commitGlobalDefault = (key) => {
    const drafts = widgetLiveEdits.getGlobalDefaultDrafts()
    if (!Object.hasOwn(drafts, key)) return
    if (key === 'scale') {
      setGlobalDefault(key, drafts.scale, undefined, scaleOriginRef.current)
      widgetLiveEdits.clearWidgetDrafts(buildConfigWidgets(useStore.getState().config).map((widget) => widget.id))
      scaleOriginRef.current = null
    } else setGlobalDefault(key, drafts[key])
    widgetLiveEdits.clearGlobalDefaultDrafts()
  }
  const resetGlobalDefaults = () =>
    prepareFontEdit(['font_text', 'font_values'], [DEFAULT_GLOBAL_DEFAULTS.font_text, DEFAULT_GLOBAL_DEFAULTS.font_values], () => {
      useStore.getState().resetGlobalDefaults(getPreparedFont(DEFAULT_GLOBAL_DEFAULTS.font_text), getScaleOrigin())
    })
  const editorConfig = useMemo(() => createEditorEffectiveConfig({ config, globalDefaults }), [config, globalDefaults])
  const scene = editorConfig?.scene ? { ...editorConfig.scene, fps: renderSettings.fps } : null
  const sceneResolutionKey = getSceneResolutionKey(scene)
  const derivedResId = getResolutionPresetId(scene)

  const [customResolutionAnchor, setCustomResolutionAnchor] = useState(null)
  const resId = customResolutionAnchor && customResolutionAnchor === sceneResolutionKey ? 'custom' : derivedResId

  const { fpsMode, handleFpsModeChange, handleCustomFpsChange } = useFpsMode({
    fps: scene?.fps,
    onFpsChange: (nextFps) => {
      setRenderFpsAndUpdateRate(nextFps, normalizeUpdateRateForFps(nextFps, renderSettings.widgetUpdateRate))
    },
    updateRate: renderSettings.widgetUpdateRate,
  })

  const updateRateOptions = useMemo(() => getUpdateRateOptions(scene?.fps), [scene?.fps])

  const videoResolutionMismatch =
    Boolean(scene?.width && scene?.height && importedVideoResolution) &&
    (Number(scene.width) !== Number(importedVideoResolution.width) || Number(scene.height) !== Number(importedVideoResolution.height))

  const sceneStyleValue = (key, fallback) => scene?.[key] ?? fallback

  const updateScene = (key, value) => {
    let finalValue = value
    if (['width', 'height', 'x', 'y', 'start', 'end'].includes(key)) finalValue = sanitizeNumber(value)
    onConfigChange({ ...config, scene: { ...config.scene, [key]: finalValue } })
  }

  const handleAspectRatioChange = (v) => {
    if (v === 'custom') {
      setCustomAspectRatio()
      return
    }

    const preset = getDefaultResolutionPreset(v)
    setAspectRatioPreset(v, { width: preset.w, height: preset.h })
  }

  const handleResolutionChange = (v) => {
    if (v === 'custom') {
      setCustomResolutionAnchor(sceneResolutionKey)
      return
    }
    setCustomResolutionAnchor(null)
    const preset = getResolutionPreset(aspectRatio, v)
    onConfigChange({ ...config, scene: { ...config.scene, width: preset.w, height: preset.h } })
  }

  const handleUpdateRateChange = (v) => setRenderWidgetUpdateRate(parseInt(v))

  const handlers = {
    handleAspectRatioChange,
    handleCustomFpsChange,
    handleFpsModeChange,
    handleResolutionChange,
    handleUpdateRateChange,
    updateScene,
  }

  return {
    overlaySettings: {
      activitySummary,
      aspectRatio,
      exportRange: renderSettings.range,
      fpsMode,
      importedVideoFps,
      importedVideoResolution,
      resId,
      scene,
      updateRate: renderSettings.widgetUpdateRate,
      updateRateOptions,
      videoResolutionMismatch,
    },
    globalSettings: {
      globalDefaults,
      resetGlobalDefaults,
      sceneStyleValue,
      setGlobalDefault: changeGlobalDefault,
      draftGlobalDefault,
      commitGlobalDefault,
      availableFonts,
    },
    handlers,
    setExportRange: setRenderRange,
    setUpdateRate: setRenderWidgetUpdateRate,
  }
}
