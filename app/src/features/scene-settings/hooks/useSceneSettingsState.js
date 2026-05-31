/**
 * @file useSceneSettingsState – Container hook for the scene settings sidebar tab.
 *
 * Orchestrates store access, local state, derived state, effects, and event
 * handlers. Returns grouped objects so consumers can pass them as coherent
 * blocks instead of manually destructuring 40+ individual keys.
 *
 * Return shape:
 * - overlaySettings   scene resolution, FPS, update rate, aspect ratio
 * - videoSyncSettings imported video metadata and sync configuration
 * - globalSettings    global defaults, fonts, scene style accessor
 * - handlers          all event handlers (each section destructures what it needs)
 *
 * @module useSceneSettingsState
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { createEditorEffectiveConfig } from '@/lib/template-state'
import useStore from '@/store/useStore'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { getUpdateRateOptions, normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { RESOLUTIONS } from '../data/sceneSettingsConstants'
import { timeToSeconds, sanitizeNumber } from '../utils/sceneSettingsUtils'

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

export default function useSceneSettingsState({ config, onConfigChange }) {
  const {
    activitySummary,
    aspectRatio,
    computeVideoSync,
    exportRange,
    globalDefaults,
    importedVideoCreationTime,
    importedVideoDuration,
    importedVideoFps,
    importedVideoPath,
    importedVideoResolution,
    resetGlobalDefaults,
    setAspectRatio,
    setExportRange,
    setGlobalDefault,
    setUpdateRate,
    setVideoSyncOffset,
    setVideoSyncWarning,
    updateRate,
    videoSyncOffsetSeconds,
    videoSyncWarning,
  } = useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      aspectRatio: state.aspectRatio,
      computeVideoSync: state.computeVideoSync,
      exportRange: state.exportRange,
      globalDefaults: state.globalDefaults,
      importedVideoCreationTime: state.importedVideoCreationTime,
      importedVideoDuration: state.importedVideoDuration,
      importedVideoFps: state.importedVideoFps,
      importedVideoPath: state.importedVideoPath,
      importedVideoResolution: state.importedVideoResolution,
      resetGlobalDefaults: state.resetGlobalDefaults,
      setAspectRatio: state.setAspectRatio,
      setExportRange: state.setExportRange,
      setGlobalDefault: state.setGlobalDefault,
      setUpdateRate: state.setUpdateRate,
      setVideoSyncOffset: state.setVideoSyncOffset,
      setVideoSyncWarning: state.setVideoSyncWarning,
      updateRate: state.updateRate,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncWarning: state.videoSyncWarning,
    })),
  )

  const availableFonts = useAvailableFonts()
  const editorConfig = useMemo(() => createEditorEffectiveConfig({ config, globalDefaults }), [config, globalDefaults])
  const scene = editorConfig?.scene
  const sceneResolutionKey = getSceneResolutionKey(scene)
  const derivedResId = getResolutionPresetId(scene)

  const [customResolutionAnchor, setCustomResolutionAnchor] = useState(null)
  const resId = customResolutionAnchor && customResolutionAnchor === sceneResolutionKey ? 'custom' : derivedResId

  const { fpsMode, handleFpsModeChange, handleCustomFpsChange } = useFpsMode({
    fps: scene?.fps,
    onFpsChange: (nextFps) => {
      setUpdateRate(normalizeUpdateRateForFps(nextFps, updateRate))
      updateScene('fps', nextFps)
    },
    updateRate,
  })

  const handleCustomFpsChangeEvent = useCallback((e) => handleCustomFpsChange(e.target.value), [handleCustomFpsChange])

  const updateRateOptions = useMemo(() => getUpdateRateOptions(scene?.fps), [scene?.fps])

  const [offsetInput, setOffsetInput] = useState(videoSyncOffsetSeconds?.toString() || '0')

  useEffect(() => {
    setOffsetInput(videoSyncOffsetSeconds?.toString() || '0')
  }, [videoSyncOffsetSeconds])

  useEffect(() => {
    const normalizedUpdateRate = normalizeUpdateRateForFps(scene?.fps, updateRate)
    if (normalizedUpdateRate !== updateRate) setUpdateRate(normalizedUpdateRate)
  }, [scene?.fps, setUpdateRate, updateRate])

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
    setAspectRatio(v)
    if (v !== 'custom' && RESOLUTIONS[v]) {
      const preset = RESOLUTIONS[v][0]
      onConfigChange({ ...config, scene: { ...config.scene, width: preset.w, height: preset.h } })
    }
  }

  const handleResolutionChange = (v) => {
    if (v === 'custom') {
      setCustomResolutionAnchor(sceneResolutionKey)
      return
    }
    setCustomResolutionAnchor(null)
    const preset = RESOLUTIONS[aspectRatio]?.find((r) => r.id === v)
    if (preset) onConfigChange({ ...config, scene: { ...config.scene, width: preset.w, height: preset.h } })
  }

  const handleUpdateRateChange = (v) => setUpdateRate(parseInt(v))

  const handleOffsetBlur = (val) => {
    const parsed = timeToSeconds(val)
    const rounded = Math.round(parsed * 10) / 10
    setVideoSyncOffset(rounded)
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1))
  }

  const handleIncrement = (amount) => {
    const current = timeToSeconds(offsetInput)
    const newOffset = Math.round((current + amount) * 10) / 10
    setVideoSyncOffset(newOffset)
    setOffsetInput(Number.isInteger(newOffset) ? newOffset.toString() : newOffset.toFixed(1))
  }

  const handlers = {
    handleAspectRatioChange,
    handleCustomFpsChange: handleCustomFpsChangeEvent,
    handleFpsModeChange,
    handleIncrement,
    handleOffsetBlur,
    handleResolutionChange,
    handleUpdateRateChange,
    updateScene,
  }

  return {
    overlaySettings: {
      activitySummary,
      aspectRatio,
      exportRange,
      fpsMode,
      importedVideoFps,
      importedVideoPath,
      resId,
      scene,
      updateRate,
      updateRateOptions,
    },
    videoSyncSettings: {
      activitySummary,
      computeVideoSync,
      importedVideoCreationTime,
      importedVideoDuration,
      importedVideoFps,
      importedVideoPath,
      importedVideoResolution,
      offsetInput,
      setOffsetInput,
      videoResolutionMismatch,
      videoSyncOffsetSeconds,
      videoSyncWarning,
    },
    globalSettings: {
      globalDefaults,
      resetGlobalDefaults,
      sceneStyleValue,
      setGlobalDefault,
      availableFonts,
    },
    handlers,
    // Store actions exposed directly for callers that need them
    setAspectRatio,
    setExportRange,
    setUpdateRate,
  }
}
