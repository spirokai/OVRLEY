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

import { useCallback, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import useStore from '@/store/useStore'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { getUpdateRateOptions, normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { RESOLUTIONS } from '../data/sceneSettingsConstants'
import { sanitizeNumber } from '../utils/sceneSettingsUtils'

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

export default function useSceneSettingsState({ config, onConfigChange }) {
  const {
    activitySummary,
    aspectRatio,
    globalDefaults,
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

  const availableFonts = useAvailableFonts()
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

  const handleCustomFpsChangeEvent = useCallback((e) => handleCustomFpsChange(e.target.value), [handleCustomFpsChange])

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
    handleCustomFpsChange: handleCustomFpsChangeEvent,
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
      resetGlobalDefaults: useStore((state) => state.resetGlobalDefaults),
      sceneStyleValue,
      setGlobalDefault,
      availableFonts,
    },
    handlers,
    setExportRange: setRenderRange,
    setUpdateRate: setRenderWidgetUpdateRate,
  }
}
