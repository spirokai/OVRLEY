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
  const [renderDialogPhase, setRenderDialogPhase] = useState('closed')
  const [renderSettingsDraft, setRenderSettingsDraft] = useState(null)

  const hasParsedActivity = Boolean(activitySummary)
  const canRender = Boolean(config && hasParsedActivity)
  const renderDisabled =
    !canRender || renderingVideo || backendStatus !== 'connected'
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

    if (renderingVideo) {
      return 'Rendering already in progress'
    }

    return null
  }, [backendStatus, config, hasParsedActivity, renderingVideo])

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

    const unsubscribe = useStore.subscribe((state, previousState) => {
      const nextProgress = state.renderProgress
      const previousProgress = previousState.renderProgress
      if (nextProgress === previousProgress) {
        return
      }

      const { activeRenderId: nextActiveRenderId } = state
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
    })

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
    return {
      fps,
      updateRate: normalizeUpdateRateForFps(fps, updateRate),
      exportCodec: exportCodec || 'prores_ks',
      exportRange: {
        ...DEFAULT_EXPORT_RANGE,
        ...(exportRange || {}),
      },
    }
  }, [config?.scene?.fps, updateRate, exportCodec, exportRange])

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
    setExportCodec(renderSettingsDraft.exportCodec)
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
