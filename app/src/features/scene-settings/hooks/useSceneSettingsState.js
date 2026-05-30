/**
 * Container hook for the scene settings sidebar tab.
 * Orchestrates store access, local state, derived state, effects, and event handlers.
 *
 * @param {object} props
 * @param {object} props.config - Overlay template configuration.
 * @param {function} props.onConfigChange - Callback to update the config.
 * @returns {object} State, derived values, and handlers for rendering.
 */

import { useEffect, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { createEditorEffectiveConfig } from '@/lib/template-state'
import useStore from '@/store/useStore'
import useAvailableFonts from '@/hooks/useAvailableFonts'
import { getFpsModeValue, getUpdateRateOptions, normalizeUpdateRateForFps, PRESET_FPS_VALUES, sanitizeIntegerFps } from '@/lib/update-rate'
import { RESOLUTIONS } from '../data/sceneSettingsConstants'
import { parseTimeOffset, sanitizeNumber } from '../utils/sceneSettingsUtils'

function getResolutionPresetId(scene) {
  if (!scene) {
    return '1080p'
  }

  const match = Object.values(RESOLUTIONS)
    .flat()
    .find((resolution) => resolution.w === scene.width && resolution.h === scene.height)

  return match ? match.id : 'custom'
}

function getSceneResolutionKey(scene) {
  if (!scene) {
    return null
  }

  return `${Number(scene.width)}x${Number(scene.height)}`
}

export default function useSceneSettingsState({ config, onConfigChange }) {
  // Store selectors
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

  // Derived state (from props)
  const systemFonts = useAvailableFonts()
  const editorConfig = useMemo(() => createEditorEffectiveConfig({ config, globalDefaults }), [config, globalDefaults])
  const scene = editorConfig?.scene
  const sceneResolutionKey = getSceneResolutionKey(scene)
  const derivedResId = getResolutionPresetId(scene)
  const derivedFpsMode = getFpsModeValue(scene?.fps)

  // Local UI state remains only for transient modes that intentionally diverge
  // from committed scene values until the relevant field actually changes.
  const [customResolutionAnchor, setCustomResolutionAnchor] = useState(null)
  const [customFpsAnchor, setCustomFpsAnchor] = useState(null)
  const resId = customResolutionAnchor && customResolutionAnchor === sceneResolutionKey ? 'custom' : derivedResId
  const fpsMode = customFpsAnchor !== null && Number(scene?.fps) === customFpsAnchor ? 'custom' : derivedFpsMode

  const updateRateOptions = useMemo(() => getUpdateRateOptions(scene?.fps), [scene?.fps])

  const [offsetInput, setOffsetInput] = useState(videoSyncOffsetSeconds?.toString() || '0')

  // Side effects — sync local input state when store values change and normalize update rate on FPS changes
  useEffect(() => {
    setOffsetInput(videoSyncOffsetSeconds?.toString() || '0')
  }, [videoSyncOffsetSeconds])

  useEffect(() => {
    const normalizedUpdateRate = normalizeUpdateRateForFps(scene?.fps, updateRate)
    if (normalizedUpdateRate !== updateRate) {
      setUpdateRate(normalizedUpdateRate)
    }
  }, [scene?.fps, setUpdateRate, updateRate])

  // Handlers — scene config mutations (aspect ratio, resolution, FPS, widget update rate, sync offset)
  const videoResolutionMismatch =
    Boolean(scene?.width && scene?.height && importedVideoResolution) &&
    (Number(scene.width) !== Number(importedVideoResolution.width) || Number(scene.height) !== Number(importedVideoResolution.height))

  const sceneStyleValue = (key, fallback) => scene?.[key] ?? fallback

  const updateScene = (key, value) => {
    let finalValue = value
    if (['width', 'height', 'x', 'y', 'start', 'end'].includes(key)) {
      finalValue = sanitizeNumber(value)
    }
    onConfigChange({ ...config, scene: { ...config.scene, [key]: finalValue } })
  }

  const handleAspectRatioChange = (v) => {
    setAspectRatio(v)
    if (v !== 'custom' && RESOLUTIONS[v]) {
      const preset = RESOLUTIONS[v][0]
      onConfigChange({
        ...config,
        scene: {
          ...config.scene,
          width: preset.w,
          height: preset.h,
        },
      })
    }
  }

  const handleResolutionChange = (v) => {
    if (v === 'custom') {
      setCustomResolutionAnchor(sceneResolutionKey)
      return
    }

    setCustomResolutionAnchor(null)
    const preset = RESOLUTIONS[aspectRatio]?.find((r) => r.id === v)
    if (preset) {
      onConfigChange({
        ...config,
        scene: {
          ...config.scene,
          width: preset.w,
          height: preset.h,
        },
      })
    }
  }

  const handleFpsModeChange = (v) => {
    if (v === 'custom') {
      setCustomFpsAnchor(Number(scene?.fps))
      return
    }

    setCustomFpsAnchor(null)
    if (v !== 'custom') {
      const fps = sanitizeIntegerFps(v)
      setUpdateRate(normalizeUpdateRateForFps(fps, updateRate))
      updateScene('fps', fps)
    }
  }

  const handleCustomFpsChange = (e) => {
    const fps = sanitizeIntegerFps(e.target.value)
    setCustomFpsAnchor(PRESET_FPS_VALUES.includes(fps) ? null : fps)
    setUpdateRate(normalizeUpdateRateForFps(fps, updateRate))
    updateScene('fps', fps)
  }

  const handleUpdateRateChange = (v) => setUpdateRate(parseInt(v))

  const handleOffsetBlur = (val) => {
    const parsed = parseTimeOffset(val)
    const rounded = Math.round(parsed * 10) / 10
    setVideoSyncOffset(rounded)
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1))
  }

  const handleIncrement = (amount) => {
    const current = parseTimeOffset(offsetInput)
    const newOffset = Math.round((current + amount) * 10) / 10
    setVideoSyncOffset(newOffset)
    setOffsetInput(Number.isInteger(newOffset) ? newOffset.toString() : newOffset.toFixed(1))
  }

  return {
    // Store state
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
    updateRate,
    videoSyncOffsetSeconds,
    videoSyncWarning,

    // Store actions
    resetGlobalDefaults,
    setAspectRatio,
    setExportRange,
    setGlobalDefault,
    setUpdateRate,
    setVideoSyncOffset,
    setVideoSyncWarning,

    // Derived
    scene,
    systemFonts,
    updateRateOptions,
    videoResolutionMismatch,
    sceneStyleValue,

    // Local state
    resId,
    fpsMode,
    offsetInput,
    setOffsetInput,

    // Handlers
    handleAspectRatioChange,
    handleCustomFpsChange,
    handleFpsModeChange,
    handleIncrement,
    handleOffsetBlur,
    handleResolutionChange,
    handleUpdateRateChange,
    updateScene,
  }
}
