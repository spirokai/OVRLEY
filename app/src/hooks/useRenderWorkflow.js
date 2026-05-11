/**
 * Implements the use Render Workflow hook and related behavior for the app.
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import * as backend from '@/api/backend'
import { useRenderStore } from '@/hooks/useAppStoreSelectors'
import { DEFAULT_EXPORT_RANGE } from '@/lib/template-snapshot'
import {
  normalizeUpdateRateForFps,
  sanitizeIntegerFps,
} from '@/lib/update-rate'
import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { getDefaultBitrate } from '@/lib/bitrateDefaults'

function resolutionsMismatch(scene, videoResolution) {
  if (!scene?.width || !scene?.height || !videoResolution) {
    return false
  }

  return (
    Number(scene.width) !== Number(videoResolution.width) ||
    Number(scene.height) !== Number(videoResolution.height)
  )
}

/**
 * Provides render workflow state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.backendStatus - Current backend status.
 * @returns {object} Result produced by the helper.
 */
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
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoResolution = useStore(
    (state) => state.importedVideoResolution,
  )
  const [renderDialogPhase, setRenderDialogPhase] = useState('closed')
  const [renderSettingsDraft, setRenderSettingsDraft] = useState(null)

  const hasParsedActivity = Boolean(activitySummary)
  const canRender = Boolean(config && hasParsedActivity)
  const hasResolutionMismatch = resolutionsMismatch(
    config?.scene,
    importedVideoResolution,
  )
  const renderDisabled =
    !canRender ||
    renderingVideo ||
    backendStatus !== 'connected' ||
    hasResolutionMismatch
  const renderTooltipContent = useMemo(() => {
    if (!config) {
      return hasParsedActivity
        ? 'Load a template first'
        : 'Load a template and GPX/FIT activity first'
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
  }, [
    backendStatus,
    config,
    hasParsedActivity,
    hasResolutionMismatch,
    renderingVideo,
  ])

  useEffect(() => {
    if (
      renderDialogPhase === 'progress' &&
      !renderingVideo &&
      ['complete', 'cancelled', 'error'].includes(renderStatus)
    ) {
      setRenderDialogPhase('closed')
    }
  }, [renderDialogPhase, renderStatus, renderingVideo])

  useEffect(() => {
    if (!renderingVideo) return

    const pollProgress = async () => {
      try {
        const data = await backend.getRenderProgress()
        const expectedRenderId = useStore.getState().activeRenderId
        if (
          expectedRenderId === null ||
          expectedRenderId === undefined ||
          data.render_id !== expectedRenderId
        ) {
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
  }, [
    renderingVideo,
    setErrorMessage,
    setActiveRenderId,
    setRenderingVideo,
    setVideoFilename,
  ])

  const buildRenderSettingsDraft = useCallback(() => {
    const fps = sanitizeIntegerFps(config?.scene?.fps || 30)
    const defaultCodec = importedVideoPath
      ? 'libx264'
      : exportCodec || 'prores_ks'
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
      exportRange: {
        ...DEFAULT_EXPORT_RANGE,
        ...(exportRange || {}),
      },
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

  const openRenderDialog = useCallback(() => {
    if (renderDisabled) {
      return
    }

    setRenderSettingsDraft(buildRenderSettingsDraft())
    setRenderDialogPhase('confirm')
  }, [buildRenderSettingsDraft, renderDisabled])

  const closeRenderDialog = useCallback(() => {
    if (renderDialogPhase === 'progress' || renderingVideo) {
      return
    }

    setRenderDialogPhase('closed')
  }, [renderDialogPhase, renderingVideo])

  const updateRenderSettingsDraft = useCallback((updates) => {
    setRenderSettingsDraft((currentDraft) => {
      if (!currentDraft) {
        return currentDraft
      }

      return {
        ...currentDraft,
        ...updates,
      }
    })
  }, [])

  const handleRenderVideoConfirm = useCallback(async () => {
    if (!config?.scene || !renderSettingsDraft) {
      return
    }

    const nextExportRange = {
      ...DEFAULT_EXPORT_RANGE,
      ...(renderSettingsDraft.exportRange || {}),
    }
    const nextFps = sanitizeIntegerFps(renderSettingsDraft.fps || 30)
    const nextUpdateRate = normalizeUpdateRateForFps(
      nextFps,
      renderSettingsDraft.updateRate,
    )
    const nextConfig = {
      ...config,
      scene: {
        ...config.scene,
        fps: nextFps,
      },
    }

    setConfig(nextConfig)
    setUpdateRate(nextUpdateRate)
    if (!useStore.getState().importedVideoPath) {
      setExportCodec(renderSettingsDraft.exportCodec)
    }
    setExportRange(nextExportRange)
    setActiveRenderId(null)
    setRenderProgress({
      ...DEFAULT_RENDER_PROGRESS,
      status: 'rendering',
      message: 'Starting render...',
    })
    setRenderingVideo(true)
    setRenderDialogPhase('progress')

    try {
      const { default: renderVideo } = await import('../api/renderVideo')
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
