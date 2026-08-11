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
import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import useStore from '@/store/useStore'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { getUpdateRateOptions, normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { RESOLUTIONS } from '../data/sceneSettingsConstants'
import { parseVideoFilenameCreationTime, timeToSeconds, sanitizeNumber } from '../utils/sceneSettingsUtils'

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

function formatOffsetInput(seconds) {
  const rounded = Math.round(seconds * 10) / 10
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1)
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
    computeVideoSync,
    exportRange,
    globalDefaults,
    parsedActivity,
    importedVideoBitRate,
    importedVideoCameraType,
    importedVideoCameraModel,
    importedVideoCodecName,
    importedVideoCodecLongName,
    importedVideoCreationTime,
    importedVideoTimeSource,
    importedVideoDuration,
    importedVideoFps,
    importedVideoPath,
    importedVideoResolution,
    resetGlobalDefaults,
    resetVideoCreationTime,
    setAspectRatioPreset,
    setCustomAspectRatio,
    setExportRange,
    setGlobalDefault,
    setSceneFpsAndUpdateRate,
    setUpdateRate,
    setVideoCreationTimeFromFilename,
    setVideoSyncOffset,
    setVideoSyncWarning,
    setVideoSyncTimezoneMode,
    updateRate,
    videoSyncTimezoneMode,
    videoSyncOffsetSeconds,
    videoSyncWarning,
  } = useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      aspectRatio: state.aspectRatio,
      computeVideoSync: state.computeVideoSync,
      exportRange: state.exportRange,
      globalDefaults: state.globalDefaults,
      parsedActivity: state.parsedActivity,
      importedVideoBitRate: state.importedVideoBitRate,
      importedVideoCameraType: state.importedVideoCameraType,
      importedVideoCameraModel: state.importedVideoCameraModel,
      importedVideoCodecName: state.importedVideoCodecName,
      importedVideoCodecLongName: state.importedVideoCodecLongName,
      importedVideoCreationTime: state.importedVideoCreationTime,
      importedVideoTimeSource: state.importedVideoTimeSource,
      importedVideoDuration: state.importedVideoDuration,
      importedVideoFps: state.importedVideoFps,
      importedVideoPath: state.importedVideoPath,
      importedVideoResolution: state.importedVideoResolution,
      resetGlobalDefaults: state.resetGlobalDefaults,
      resetVideoCreationTime: state.resetVideoCreationTime,
      setAspectRatioPreset: state.setAspectRatioPreset,
      setCustomAspectRatio: state.setCustomAspectRatio,
      setExportRange: state.setExportRange,
      setGlobalDefault: state.setGlobalDefault,
      setSceneFpsAndUpdateRate: state.setSceneFpsAndUpdateRate,
      setUpdateRate: state.setUpdateRate,
      setVideoCreationTimeFromFilename: state.setVideoCreationTimeFromFilename,
      setVideoSyncOffset: state.setVideoSyncOffset,
      setVideoSyncWarning: state.setVideoSyncWarning,
      setVideoSyncTimezoneMode: state.setVideoSyncTimezoneMode,
      updateRate: state.updateRate,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncWarning: state.videoSyncWarning,
    })),
  )

  const timezone = parsedActivity?.metadata?.timezone ?? null

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
      setSceneFpsAndUpdateRate(nextFps, normalizeUpdateRateForFps(nextFps, updateRate))
    },
    updateRate,
  })

  const handleCustomFpsChangeEvent = useCallback((e) => handleCustomFpsChange(e.target.value), [handleCustomFpsChange])

  const updateRateOptions = useMemo(() => getUpdateRateOptions(scene?.fps), [scene?.fps])

  const [offsetInput, setOffsetInput] = useState(formatOffsetInput(videoSyncOffsetSeconds ?? 0))

  useEffect(() => {
    setOffsetInput(formatOffsetInput(videoSyncOffsetSeconds ?? 0))
  }, [videoSyncOffsetSeconds])

  useEffect(() => {
    const normalizedUpdateRate = normalizeUpdateRateForFps(scene?.fps, updateRate)
    if (normalizedUpdateRate !== updateRate) setUpdateRate(normalizedUpdateRate)
  }, [scene?.fps, setUpdateRate, updateRate])

  const videoResolutionMismatch =
    Boolean(scene?.width && scene?.height && importedVideoResolution) &&
    (Number(scene.width) !== Number(importedVideoResolution.width) || Number(scene.height) !== Number(importedVideoResolution.height))
  const filenameCreationTimeAvailable = parseVideoFilenameCreationTime(importedVideoPath) !== null

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

  const handleUpdateRateChange = (v) => setUpdateRate(parseInt(v))

  const handleOffsetBlur = (val) => {
    const parsed = timeToSeconds(val)
    const rounded = Math.round(parsed * 10) / 10
    try {
      setVideoSyncOffset(rounded)
    } catch (error) {
      setVideoSyncWarning(error.message)
      setOffsetInput(formatOffsetInput(videoSyncOffsetSeconds ?? 0))
      return
    }
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1))
  }

  const handleIncrement = (amount) => {
    const current = timeToSeconds(offsetInput)
    const newOffset = Math.round((current + amount) * 10) / 10
    try {
      setVideoSyncOffset(newOffset)
    } catch (error) {
      setVideoSyncWarning(error.message)
      return
    }
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(newOffset) ? newOffset.toString() : newOffset.toFixed(1))
  }

  const handleComputeVideoSync = useCallback(() => {
    computeVideoSync(activitySummary)
  }, [activitySummary, computeVideoSync])

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
      resId,
      scene,
      updateRate,
      updateRateOptions,
    },
    videoSyncSettings: {
      activitySummary,
      computeVideoSync: handleComputeVideoSync,
      importedVideoBitRate,
      importedVideoCameraType,
      importedVideoCameraModel,
      importedVideoCodecName,
      importedVideoCodecLongName,
      importedVideoCreationTime,
      importedVideoTimeSource,
      timezone,
      importedVideoDuration,
      importedVideoFps,
      importedVideoPath,
      importedVideoResolution,
      filenameCreationTimeAvailable,
      canResetVideoCreationTime: importedVideoTimeSource === 'filename',
      offsetInput,
      setVideoCreationTimeFromFilename,
      resetVideoCreationTime,
      setOffsetInput,
      setVideoSyncTimezoneMode,
      videoResolutionMismatch,
      videoSyncTimezoneMode,
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
    setExportRange,
    setUpdateRate,
  }
}
