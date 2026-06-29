/**
 * Orchestrates the render video workflow by composing dialog state,
 * progress polling, and completion handling.
 *
 * @param {object} options
 * @param {string} options.backendStatus - Current backend connection status.
 * @returns {object} Render workflow API for use by AppShell.
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import * as backend from '@/api/backend'
import { useRenderStore } from '@/hooks/useAppStoreSelectors'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import { resolvePreviewSecond } from '@/lib/preview-timing'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { getDefaultBitrate } from '../data/bitrateDefaults'
import { createRenderEffectiveConfig } from '../utils/renderConfig'
import useRenderDialogState from './useRenderDialogState'

export default function useRenderWorkflow({ backendStatus }) {
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
  const globalDefaults = useStore((state) => state.globalDefaults)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoResolution = useStore((state) => state.importedVideoResolution)
  const [renderingPreviewFrame, setRenderingPreviewFrame] = useState(false)

  const hasParsedActivity = Boolean(activitySummary)
  const canRender = Boolean(config && hasParsedActivity)
  const renderDisabled = !canRender || renderingVideo || backendStatus !== 'connected'
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

    if (renderingVideo) {
      return 'Rendering already in progress'
    }
    return null
  }, [backendStatus, config, hasParsedActivity, renderingVideo])
  const renderPreviewFrameDisabled = !canRender || renderingVideo || renderingPreviewFrame || backendStatus !== 'connected'

  const buildRenderSettingsDraft = useCallback(() => {
    const templateFps = sanitizeIntegerFps(config?.scene?.fps || 30)
    const fps = importedVideoPath && importedVideoFps ? sanitizeIntegerFps(Math.round(importedVideoFps)) : templateFps
    const defaultCodec = importedVideoPath ? 'libx264' : exportCodec || 'prores_ks'
    const draftExportRange = importedVideoPath ? DEFAULT_EXPORT_RANGE : { ...DEFAULT_EXPORT_RANGE, ...(exportRange || {}) }

    return {
      fps,
      updateRate: normalizeUpdateRateForFps(fps, updateRate),
      exportMode: importedVideoPath ? 'composite' : 'transparent',
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

  // Progress polling — polls backend for render progress at 500ms intervals while rendering is active
  useEffect(() => {
    if (!renderingVideo) return

    const pollProgress = async () => {
      try {
        const data = await backend.getRenderProgress()
        const expectedRenderId = useStore.getState().activeRenderId
        if (expectedRenderId === null || expectedRenderId === undefined || data.render_id !== expectedRenderId) {
          return
        }

        setRenderProgress({
          renderId: data.render_id ?? null,
          current: data.current || 0,
          total: data.total || 0,
          encoded: data.encoded || 0,
          status: data.status || 'rendering',
          message: data.message || '',
          estimatedSecondsRemaining: data.estimated_seconds_remaining,
          renderingFps: data.rendering_fps ?? null,
          filename: data.filename || null,
        })
      } catch (error) {
        console.error('Error polling render progress:', error)
      }
    }

    const interval = setInterval(pollProgress, 500)
    pollProgress()
    return () => clearInterval(interval)
  }, [renderingVideo, setRenderProgress])

  // Render completion handler — subscribes to render progress store to handle completion, cancellation, and errors
  useEffect(() => {
    if (!renderingVideo) return

    const unsubscribe = useStore.subscribe(
      (state) => state.renderProgress,
      (nextProgress) => {
        const { activeRenderId: nextActiveRenderId } = useStore.getState()
        if (nextProgress.renderId !== nextActiveRenderId) {
          return
        }

        const { filename, message, status } = nextProgress

        if (status === 'complete' && filename) {
          setVideoFilename(filename)
          setActiveRenderId(null)
          setRenderingVideo(false)
          backend.openVideo(filename).catch((error) => {
            console.error('Error calling open-video:', error)
          })
          return
        }

        if (status === 'cancelled') {
          setActiveRenderId(null)
          setRenderingVideo(false)
          return
        }

        if (status === 'error') {
          setActiveRenderId(null)
          setRenderingVideo(false)
          if (message) {
            setErrorMessage(message)
          }
        }
      },
    )

    return unsubscribe
  }, [renderingVideo, setErrorMessage, setActiveRenderId, setRenderingVideo, setVideoFilename])

  // Confirm handler — persists dialog-local render choices, resolves the active export
  // pipeline, kicks off the render IPC call, and manages error/recovery flow.
  const handleRenderVideoConfirm = useCallback(async () => {
    if (!config?.scene || !renderSettingsDraft) {
      return
    }

    // The dialog draft is the source of truth once opened; imported video only
    // provides the default when a draft does not yet carry an explicit mode.
    const exportMode = renderSettingsDraft.exportMode || (useStore.getState().importedVideoPath ? 'composite' : 'transparent')
    const shouldComposite = exportMode === 'composite'
    const nextExportRange = shouldComposite
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
    if (!shouldComposite) {
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
        exportMode,
        updateRate: nextUpdateRate,
        exportRange: nextExportRange,
        exportCodec: renderSettingsDraft.exportCodec,
        exportBitrate: renderSettingsDraft.exportBitrate,
        availableCodecs: useStore.getState().availableCodecs,
        globalDefaults,
        importedVideoDuration: useStore.getState().importedVideoDuration,
        importedVideoFps: useStore.getState().importedVideoFps,
        importedVideoFpsDen: useStore.getState().importedVideoFpsDen,
        importedVideoFpsNum: useStore.getState().importedVideoFpsNum,
        importedVideoPath: shouldComposite ? useStore.getState().importedVideoPath : null,
        importedVideoResolution: useStore.getState().importedVideoResolution,
        parsedActivity: useStore.getState().parsedActivity,
        startSecond: useStore.getState().startSecond,
        endSecond: useStore.getState().endSecond,
        videoSyncOffsetSeconds: useStore.getState().videoSyncOffsetSeconds,
        setActiveRenderId,
        setRenderingVideo,
        setRenderProgress,
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
    globalDefaults,
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

  const handleRenderPreviewFrame = useCallback(async () => {
    if (renderPreviewFrameDisabled || !config?.scene) {
      return
    }

    try {
      const parsedActivity = useStore.getState().parsedActivity
      if (!parsedActivity) {
        throw new Error('No parsed activity available')
      }

      setRenderingPreviewFrame(true)
      const nextConfig = createRenderEffectiveConfig({
        availableCodecs: useStore.getState().availableCodecs,
        config,
        exportBitrate: renderSettingsDraft?.exportBitrate,
        exportCodec,
        exportRange,
        globalDefaults,
        importedVideoDuration: useStore.getState().importedVideoDuration,
        importedVideoFps: useStore.getState().importedVideoFps,
        importedVideoFpsDen: useStore.getState().importedVideoFpsDen,
        importedVideoFpsNum: useStore.getState().importedVideoFpsNum,
        importedVideoPath: useStore.getState().importedVideoPath,
        importedVideoResolution: useStore.getState().importedVideoResolution,
        timelineEnd: useStore.getState().endSecond,
        timelineStart: useStore.getState().startSecond,
        updateRate,
        videoSyncOffsetSeconds: useStore.getState().videoSyncOffsetSeconds,
      })
      const previewFps = sanitizeIntegerFps(nextConfig.scene.fps || 30)

      const dummyDurationSeconds = useStore.getState().dummyDurationSeconds
      const selectedSecond = useStore.getState().selectedSecond
      const resolvedPreviewSecond = resolvePreviewSecond({
        dummyDurationSeconds,
        selectedSecond,
        sourceActivity: parsedActivity,
      })
      const previewSecond = Math.min(Math.max(resolvedPreviewSecond, nextConfig.scene.start ?? 0), nextConfig.scene.end ?? resolvedPreviewSecond)

      nextConfig.scene = {
        ...nextConfig.scene,
        fps: previewFps,
        update_rate: normalizeUpdateRateForFps(previewFps, updateRate),
      }
      delete nextConfig.scene.updateRate

      const result = await backend.renderPreviewFrame(nextConfig, parsedActivity, previewSecond)
      if (result?.filename) {
        try {
          await backend.openVideo(result.filename)
        } catch (openError) {
          console.warn('Preview frame rendered, but opening the output failed:', openError)
        }
      }
    } catch (error) {
      console.error('Preview frame render failed:', error)
      setErrorMessage(error.message || 'Failed to render preview frame')
    } finally {
      setRenderingPreviewFrame(false)
    }
  }, [config, exportCodec, exportRange, globalDefaults, renderPreviewFrameDisabled, renderSettingsDraft?.exportBitrate, setErrorMessage, updateRate])

  return {
    closeRenderDialog,
    handleRenderPreviewFrame,
    handleRenderVideoConfirm,
    openRenderDialog,
    renderDialogPhase,
    renderDisabled,
    renderPreviewFrameDisabled,
    renderSettingsDraft,
    renderTooltipContent,
    renderingVideo,
    updateRenderSettingsDraft,
  }
}
