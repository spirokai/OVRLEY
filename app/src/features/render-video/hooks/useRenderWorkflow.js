/**
 * Orchestrates the render video workflow by composing sub-hooks for
 * dialog state, progress polling, and completion handling.
 *
 * @param {object} options
 * @param {string} options.backendStatus - Current backend connection status.
 * @returns {object} Render workflow API for use by AppShell.
 */

import { useCallback, useMemo } from 'react'
import { useRenderStore } from '@/hooks/useAppStoreSelectors'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { getDefaultBitrate } from '../utils/bitrateDefaults'
import { resolutionsMismatch } from '../utils/codecUtils'
import useRenderDialogState from './useRenderDialogState'
import useRenderProgressPolling from './useRenderProgressPolling'
import useRenderCompletion from './useRenderCompletion'

export default function useRenderWorkflow({ backendStatus }) {
  // Store selectors
  const {
    activitySummary,
    config,
    exportCodec,
    exportRange,
    renderStatus,
    renderingVideo,
    setActiveRenderId,
    setConfig,
    setErrorMessage,
    setExportCodec,
    setExportRange,
    setRenderProgress,
    setRenderingVideo,
    setUpdateRate,
    setVideoFilename,
    updateRate,
  } = useRenderStore()
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoResolution = useStore((state) => state.importedVideoResolution)

  // Derived state — computed flags and tooltip messages for render readiness
  const hasParsedActivity = Boolean(activitySummary)
  const canRender = Boolean(config && hasParsedActivity)
  const hasResolutionMismatch = resolutionsMismatch(config?.scene, importedVideoResolution)
  const renderDisabled = !canRender || renderingVideo || backendStatus !== 'connected' || hasResolutionMismatch
  const renderTooltipContent = useMemo(() => {
    if (!config) {
      return hasParsedActivity ? 'Load a template first' : 'Load a template and GPX/FIT activity first'
    }
    if (!hasParsedActivity) {
      return 'Load a GPX/FIT activity first'
    }
    if (backendStatus !== 'connected') {
      return 'Backend offline'
    }
    if (hasResolutionMismatch) {
      return 'Overlay and imported video resolutions must match'
    }
    if (renderingVideo) {
      return 'Rendering already in progress'
    }
    return null
  }, [backendStatus, config, hasParsedActivity, hasResolutionMismatch, renderingVideo])

  // Build render settings draft — assembles initial FPS, codec, bitrate, and export range from store
  const buildRenderSettingsDraft = useCallback(() => {
    const templateFps = sanitizeIntegerFps(config?.scene?.fps || 30)
    const fps = importedVideoPath && importedVideoFps ? sanitizeIntegerFps(Math.round(importedVideoFps)) : templateFps
    const defaultCodec = importedVideoPath ? 'libx264' : exportCodec || 'prores_ks'
    const draftExportRange = importedVideoPath ? DEFAULT_EXPORT_RANGE : { ...DEFAULT_EXPORT_RANGE, ...(exportRange || {}) }

    return {
      fps,
      updateRate: normalizeUpdateRateForFps(fps, updateRate),
      exportCodec: defaultCodec,
      exportBitrate: importedVideoPath
        ? getDefaultBitrate(
            importedVideoResolution?.width || config?.scene?.width,
            importedVideoResolution?.height || config?.scene?.height,
            importedVideoFps || fps,
            defaultCodec,
          )
        : undefined,
      exportRange: draftExportRange,
    }
  }, [
    config?.scene?.fps,
    config?.scene?.height,
    config?.scene?.width,
    exportCodec,
    exportRange,
    importedVideoFps,
    importedVideoPath,
    importedVideoResolution?.height,
    importedVideoResolution?.width,
    updateRate,
  ])

  const { renderDialogPhase, renderSettingsDraft, setRenderDialogPhase, openRenderDialog, closeRenderDialog, updateRenderSettingsDraft } =
    useRenderDialogState({
      buildRenderSettingsDraft,
      renderDisabled,
      renderingVideo,
      renderStatus,
    })

  useRenderProgressPolling({ renderingVideo, setRenderProgress })

  useRenderCompletion({
    renderingVideo,
    setActiveRenderId,
    setRenderingVideo,
    setErrorMessage,
    setVideoFilename,
  })

  // Confirm handler — persists render settings, kicks off the render IPC call, and manages error/recovery flow
  const handleRenderVideoConfirm = useCallback(async () => {
    if (!config?.scene || !renderSettingsDraft) {
      return
    }

    const hasImportedVideo = Boolean(useStore.getState().importedVideoPath)
    const nextExportRange = hasImportedVideo
      ? DEFAULT_EXPORT_RANGE
      : {
          ...DEFAULT_EXPORT_RANGE,
          ...(renderSettingsDraft.exportRange || {}),
        }
    const nextFps = sanitizeIntegerFps(renderSettingsDraft.fps || 30)
    const nextUpdateRate = normalizeUpdateRateForFps(nextFps, renderSettingsDraft.updateRate)
    const nextConfig = {
      ...config,
      scene: {
        ...config.scene,
        fps: nextFps,
      },
    }

    setConfig(nextConfig)
    setUpdateRate(nextUpdateRate)
    if (!hasImportedVideo) {
      setExportCodec(renderSettingsDraft.exportCodec)
      setExportRange(nextExportRange)
    }
    setActiveRenderId(null)
    setRenderProgress({
      ...DEFAULT_RENDER_PROGRESS,
      status: 'rendering',
      message: 'Starting render...',
    })
    setRenderingVideo(true)
    setRenderDialogPhase('progress')

    try {
      const { default: renderVideo } = await import('@/features/render-video/utils/render-video')
      const result = await renderVideo({
        config: nextConfig,
        updateRate: nextUpdateRate,
        exportRange: nextExportRange,
        exportCodec: renderSettingsDraft.exportCodec,
        exportBitrate: renderSettingsDraft.exportBitrate,
      })
      if (result && result.cancelled) {
        console.log('Render video cancelled (UI handled)')
      }
    } catch (error) {
      setRenderDialogPhase('closed')
      console.error('Render failed:', error)
      useStore.getState().setErrorMessage(error.message || 'Unknown error')
    }
  }, [
    config,
    renderSettingsDraft,
    setConfig,
    setActiveRenderId,
    setExportCodec,
    setExportRange,
    setRenderProgress,
    setRenderingVideo,
    setUpdateRate,
    setRenderDialogPhase,
  ])

  return {
    closeRenderDialog,
    handleRenderVideoConfirm,
    openRenderDialog,
    renderDialogPhase,
    renderDisabled,
    renderSettingsDraft,
    renderTooltipContent,
    renderingVideo,
    updateRenderSettingsDraft,
  }
}
