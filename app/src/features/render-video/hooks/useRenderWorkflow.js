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
import { buildPreviewFrameWindow } from '@/lib/preview-timing'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { createRenderEffectiveConfig } from '../utils/renderConfig'
import { loadRememberedRenderDirectory, normalizeRenderOutputPath, rememberAcceptedRenderOutput } from '../utils/render-output'
import useRenderDialogState from './useRenderDialogState'
import i18next from 'i18next'

export default function useRenderWorkflow({ backendStatus }) {
  const {
    activitySummary,
    config,
    renderSettings,
    renderStatus,
    renderingVideo,
    clearRenderSession,
    setErrorMessage,
    setRenderProgress,
    startRenderSession,
    setRenderSettings,
  } = useRenderStore()
  const globalDefaults = useStore((state) => state.globalDefaults)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const [renderingPreviewFrame, setRenderingPreviewFrame] = useState(false)
  const [submissionPending, setSubmissionPending] = useState(false)
  const [outputPathError, setOutputPathError] = useState(null)
  const [overwriteOpen, setOverwriteOpen] = useState(false)
  const [pendingOverwritePath, setPendingOverwritePath] = useState(null)

  const hasParsedActivity = Boolean(activitySummary)
  const canRender = Boolean(config && hasParsedActivity)
  const renderDisabled = !canRender || renderingVideo || backendStatus !== 'connected'
  const renderTooltipContent = useMemo(() => {
    if (!config) {
      return hasParsedActivity ? i18next.t('render-video.loadATemplateFirst', 'Load a template first') : i18next.t('render-video.loadATemplateAndGpxfitActivityFirst', 'Load a template and GPX/FIT activity first')
    }
    if (!hasParsedActivity) {
      return i18next.t('render-video.loadAGpxfitActivityFirst', 'Load a GPX/FIT activity first')
    }
    if (backendStatus !== 'connected') {
      return i18next.t('render-video.backendOffline', 'Backend offline')
    }

    if (renderingVideo) {
      return i18next.t('render-video.renderingAlreadyInProgress', 'Rendering already in progress')
    }
    return null
  }, [backendStatus, config, hasParsedActivity, renderingVideo])
  const renderPreviewFrameDisabled = renderDisabled || renderingPreviewFrame

  const buildRenderSettingsDraft = useCallback(() => {
    const fps = sanitizeIntegerFps(renderSettings.fps)
    const codec = renderSettings.codec
    const draftExportRange = { ...DEFAULT_EXPORT_RANGE, ...renderSettings.range }

    return {
      fps,
      updateRate: normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate),
      exportMode: importedVideoPath ? renderSettings.exportMode : 'transparent',
      exportCodec: codec,
      exportBitrate: renderSettings.bitrateMbps ?? undefined,
      exportRange: draftExportRange,
    }
  }, [importedVideoPath, renderSettings])

  const resolveRenderSettingsDraft = useCallback(async () => {
    const draft = buildRenderSettingsDraft()
    const outputKind = draft.exportMode === 'composite' ? 'composite' : 'transparent'
    const rememberedDirectory = await loadRememberedRenderDirectory()
    const outputPath = await backend.suggestRenderOutputPath(outputKind, rememberedDirectory)
    return {
      ...draft,
      outputPath,
    }
  }, [buildRenderSettingsDraft])

  const {
    renderDialogPhase,
    renderSettingsDraft,
    setRenderDialogPhase,
    openRenderDialog,
    closeRenderDialog,
    updateRenderSettingsDraft: updateDraftState,
  } = useRenderDialogState({
    buildRenderSettingsDraft,
    onOpenError: (error) => setErrorMessage(error.message || 'Failed to prepare render output'),
    resolveRenderSettingsDraft,
    renderDisabled,
    renderingVideo,
    renderStatus,
    submissionPending,
  })

  const updateRenderSettingsDraft = useCallback(
    (updates) => {
      if (updates.outputPath !== undefined || updates.exportMode !== undefined) {
        setOutputPathError(null)
        setOverwriteOpen(false)
        setPendingOverwritePath(null)
      }
      updateDraftState((currentDraft) => {
        if (updates.outputPath === undefined && updates.exportMode === undefined) {
          return updates
        }

        const nextExportMode = updates.exportMode ?? currentDraft.exportMode
        const nextOutputPath = updates.outputPath ?? currentDraft.outputPath
        return nextExportMode && nextOutputPath
          ? {
              ...updates,
              outputPath: normalizeRenderOutputPath(nextOutputPath, nextExportMode),
            }
          : updates
      })
    },
    [updateDraftState],
  )

  // Progress streaming — subscribes to backend `render-progress` events for
  // live render updates. Each event carries the full RenderProgress payload
  // (current/total/encoded/status/message/eta/fps/filename) at the natural
  // frame-production cadence, replacing the previous 500 ms polling loop.
  // An initial one-shot `getRenderProgress` fetch seeds the bar before the
  // first batch wraps through the coordinator's reorder window.
  useEffect(() => {
    if (!renderingVideo) return

    let unlisten = null
    let cancelled = false

    const applyProgress = (data) => {
      if (cancelled) return
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
    }

    backend
      .subscribeRenderProgress(applyProgress)
      .then((un) => {
        if (cancelled) {
          un()
        } else {
          unlisten = un
        }
      })
      .catch((error) => {
        console.error('Failed to subscribe to render-progress events:', error)
      })

    // Seed an initial snapshot so the UI reflects the just-started render
    // immediately, before the first batch produces the first streamed event.
    backend
      .getRenderProgress()
      .then(applyProgress)
      .catch((error) => {
        console.error('Error fetching initial render progress:', error)
      })

    return () => {
      cancelled = true
      if (unlisten) unlisten()
    }
  }, [renderingVideo, setRenderProgress])

  // Render completion handler — subscribes to render progress store to handle completion, cancellation, and errors
  useEffect(() => {
    if (!renderingVideo) return

    let previousProgress = null
    const handleCompletion = (nextProgress) => {
      const { activeRenderId: nextActiveRenderId } = useStore.getState()
      if (nextProgress.renderId !== nextActiveRenderId) {
        return
      }

      const { filename, message, status } = nextProgress

      if (status === 'complete' && filename) {
        const outputPath = useStore.getState().activeRenderOutputPath
        clearRenderSession()
        if (!outputPath) {
          setErrorMessage('Completed render output path is unavailable')
          return
        }
        backend.openVideo(outputPath).catch((error) => {
          console.error('Error calling open-video:', error)
        })
        return
      }

      if (status === 'cancelled') {
        clearRenderSession()
        return
      }

      if (status === 'error') {
        clearRenderSession()
        if (message) {
          setErrorMessage(message)
        }
      }
    }
    const unsubscribe = useStore.subscribe((state) => {
      const nextProgress = state.renderProgress
      if (nextProgress === previousProgress) return
      previousProgress = nextProgress
      handleCompletion(nextProgress)
    })
    handleCompletion(useStore.getState().renderProgress)

    return unsubscribe
  }, [clearRenderSession, renderingVideo, setErrorMessage])

  // Confirm handler — resolves dialog-local render choices, kicks off the
  // render IPC call, and manages error/recovery flow. Modal choices are not
  // promoted into editor state.
  const submitRender = useCallback(
    async (overwrite = false, expectedPath = null) => {
      if (!config?.scene || !renderSettingsDraft || submissionPending) {
        return
      }

      const exportMode = renderSettingsDraft.exportMode || (useStore.getState().importedVideoPath ? 'composite' : 'transparent')
      const outputPath = renderSettingsDraft.outputPath
      if (!renderSettingsDraft.outputPath) {
        setOutputPathError('Render output path is required')
        return
      }
      if (expectedPath && outputPath !== expectedPath) {
        setPendingOverwritePath(null)
        setOverwriteOpen(false)
        return
      }
      const shouldComposite = exportMode === 'composite'
      const nextExportRange = {
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

      setSubmissionPending(true)
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
          outputPath,
          overwrite,
        })
        setRenderSettings({
          fps: nextFps,
          widgetUpdateRate: nextUpdateRate,
          exportMode,
          codec: renderSettingsDraft.exportCodec,
          bitrateMbps: renderSettingsDraft.exportBitrate ?? null,
          range: nextExportRange,
        })
        startRenderSession(result.render_id, result.outputPath, {
          ...DEFAULT_RENDER_PROGRESS,
          status: 'rendering',
          message: i18next.t('render-video.startingRender', 'Starting render...'),
        })
        setOutputPathError(null)
        setPendingOverwritePath(null)
        setOverwriteOpen(false)
        setRenderDialogPhase('progress')
        void rememberAcceptedRenderOutput(result.outputPath)
      } catch (error) {
        if (error.code === 'already_exists') {
          setPendingOverwritePath(outputPath)
          setOverwriteOpen(true)
        } else if (error.code === 'output_error') {
          setOutputPathError(error.message)
        } else {
          setRenderDialogPhase('closed')
          setErrorMessage(error.message || 'Unknown error')
        }
      } finally {
        setSubmissionPending(false)
      }
    },
    [config, globalDefaults, renderSettingsDraft, setErrorMessage, setRenderDialogPhase, setRenderSettings, startRenderSession, submissionPending],
  )

  const handleRenderVideoConfirm = useCallback(() => submitRender(false), [submitRender])

  const handleOverwriteConfirm = useCallback(() => {
    if (!pendingOverwritePath) {
      return
    }
    return submitRender(true, pendingOverwritePath)
  }, [pendingOverwritePath, submitRender])

  const handleOverwriteCancel = useCallback(() => {
    setPendingOverwritePath(null)
    setOverwriteOpen(false)
  }, [])

  const handleCloseRenderDialog = useCallback(() => {
    closeRenderDialog()
    setPendingOverwritePath(null)
    setOverwriteOpen(false)
    setOutputPathError(null)
  }, [closeRenderDialog])

  const handleRenderPreviewFrame = useCallback(async () => {
    if (renderPreviewFrameDisabled || !config?.scene) {
      return
    }

    try {
      const nextParsedActivity = useStore.getState().parsedActivity
      if (!nextParsedActivity) {
        throw new Error('No parsed activity available')
      }

      setRenderingPreviewFrame(true)
      const nextConfig = createRenderEffectiveConfig({
        availableCodecs: useStore.getState().availableCodecs,
        config,
        exportBitrate: renderSettingsDraft?.exportBitrate,
        exportCodec: renderSettings.codec,
        exportRange: renderSettings.range,
        globalDefaults,
        importedVideoDuration: useStore.getState().importedVideoDuration,
        importedVideoFps: useStore.getState().importedVideoFps,
        importedVideoFpsDen: useStore.getState().importedVideoFpsDen,
        importedVideoFpsNum: useStore.getState().importedVideoFpsNum,
        importedVideoPath: useStore.getState().importedVideoPath,
        importedVideoResolution: useStore.getState().importedVideoResolution,
        timelineEnd: useStore.getState().endSecond,
        timelineStart: useStore.getState().startSecond,
        updateRate: renderSettings.widgetUpdateRate,
        videoSyncOffsetSeconds: useStore.getState().videoSyncOffsetSeconds,
      })
      const previewFps = sanitizeIntegerFps(nextConfig.scene.fps || 30)

      const selectedSecond = useStore.getState().selectedSecond
      const sceneStart = nextConfig.scene.start ?? 0
      const sceneEnd = nextConfig.scene.end ?? sceneStart
      const previewWindow = buildPreviewFrameWindow({
        activityDuration: sceneEnd - sceneStart,
        previewSecond: selectedSecond - sceneStart,
        sceneFps: previewFps,
      })

      nextConfig.scene = {
        ...nextConfig.scene,
        start: sceneStart + previewWindow.start,
        end: sceneStart + previewWindow.end,
        fps: previewFps,
        update_rate: normalizeUpdateRateForFps(previewFps, renderSettings.widgetUpdateRate),
      }
      delete nextConfig.scene.updateRate

      const result = await backend.renderPreviewFrame(nextConfig, nextParsedActivity, selectedSecond)
      if (result?.path) {
        try {
          await backend.openVideo(result.path)
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
  }, [config, globalDefaults, renderPreviewFrameDisabled, renderSettings, renderSettingsDraft?.exportBitrate, setErrorMessage])

  return {
    closeRenderDialog: handleCloseRenderDialog,
    handleRenderPreviewFrame,
    handleRenderVideoConfirm,
    handleOverwriteCancel,
    handleOverwriteConfirm,
    openRenderDialog,
    renderDialogPhase,
    renderDisabled,
    renderPreviewFrameDisabled,
    renderSettingsDraft,
    renderTooltipContent,
    renderingVideo,
    outputPathError,
    overwriteOpen,
    pendingOverwritePath,
    submissionPending,
    updateRenderSettingsDraft,
  }
}
