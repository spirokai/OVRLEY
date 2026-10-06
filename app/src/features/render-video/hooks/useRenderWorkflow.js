import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { loadRememberedRenderDirectory } from '@/lib/file-dialog'
import { createRenderSettingsDraft, isRendererBusy } from '../utils/renderRequest'
import { normalizeRenderOutputPath } from '../utils/renderPresentation'
import useRenderExecution from './useRenderExecution'
import i18next from 'i18next'

/**
 * Owns dialog intent and routes both targets through one execution owner.
 * The dialog owns batch review and progress; execution owns accepted native sessions.
 * @param {object} options Backend connection status.
 * @returns {object} Shell actions and dialog presentation/controls.
 */
export default function useRenderWorkflow({ backendStatus }) {
  const [renderDialogPhase, setRenderDialogPhase] = useState('closed')
  const [renderSettingsDraft, setRenderSettingsDraft] = useState(null)
  const execution = useRenderExecution({ reviewTarget: renderDialogPhase === 'confirm' ? renderSettingsDraft.renderTarget : null })
  const { activitySummary, config, renderStatus, renderingVideo, rendererBusy, setErrorMessage } = execution
  const [outputPathError, setOutputPathError] = useState(null)
  const [pendingOverwritePath, setPendingOverwritePath] = useState(null)
  const opening = useRef(false)
  const hasParsedActivity = activitySummary !== null
  const renderDisabled = config === null || !hasParsedActivity || rendererBusy || backendStatus !== 'connected'
  const renderTooltipContent = useMemo(() => {
    if (config === null) {
      return hasParsedActivity
        ? i18next.t('render-video.loadATemplateFirst', 'Load a template first')
        : i18next.t('render-video.loadATemplateAndGpxfitActivityFirst', 'Load a template and GPX/FIT activity first')
    }
    if (!hasParsedActivity) return i18next.t('render-video.loadAGpxfitActivityFirst', 'Load a GPX/FIT activity first')
    if (backendStatus !== 'connected') return i18next.t('render-video.backendOffline', 'Backend offline')
    if (rendererBusy) return i18next.t('render-video.renderingAlreadyInProgress', 'Rendering already in progress')
    return null
  }, [backendStatus, rendererBusy, config, hasParsedActivity])

  useEffect(() => {
    if (renderDialogPhase === 'progress' && !renderingVideo && ['complete', 'cancelled', 'error'].includes(renderStatus)) {
      setRenderDialogPhase('closed')
    }
  }, [renderDialogPhase, renderingVideo, renderStatus])

  const clearOutputReview = useCallback(() => {
    setOutputPathError(null)
    setPendingOverwritePath(null)
  }, [])

  const openRenderDialog = useCallback(async () => {
    if (renderDisabled || opening.current || isRendererBusy(useStore.getState())) return
    opening.current = true
    try {
      const draft = createRenderSettingsDraft(useStore.getState())
      const directory = await loadRememberedRenderDirectory()
      const outputPath = await backend.suggestRenderOutputPath(draft.exportMode, directory)
      setRenderSettingsDraft({ ...draft, outputPath })
      clearOutputReview()
      setRenderDialogPhase('confirm')
    } catch (error) {
      setErrorMessage(error.message)
    } finally {
      opening.current = false
    }
  }, [clearOutputReview, renderDisabled, setErrorMessage])

  const closeRenderDialog = useCallback(() => {
    if (isRendererBusy(useStore.getState())) return
    useStore.getState().clearBatchResults()
    setRenderDialogPhase('closed')
    clearOutputReview()
  }, [clearOutputReview])

  const updateRenderSettingsDraft = useCallback(
    (updates) => {
      if (isRendererBusy(useStore.getState())) return
      if (updates.renderTarget !== undefined) useStore.getState().setRenderTarget(updates.renderTarget)
      if (updates.outputPath !== undefined || updates.exportMode !== undefined) clearOutputReview()
      setRenderSettingsDraft((draft) => {
        if (draft === null) return draft
        const next = { ...draft, ...updates }
        if ((updates.outputPath !== undefined || updates.exportMode !== undefined) && next.outputPath !== '') {
          next.outputPath = normalizeRenderOutputPath(next.outputPath, next.exportMode)
        }
        return next
      })
    },
    [clearOutputReview],
  )

  const submitRender = useCallback(
    async (batchReview, overwrite = false, expectedPath = null) => {
      if (renderSettingsDraft === null || isRendererBusy(useStore.getState()) || backendStatus !== 'connected') return
      const target = renderSettingsDraft.renderTarget
      if (target === 'batch' && batchReview === null) return
      if (target === 'current' && !renderSettingsDraft.outputPath) {
        setOutputPathError('Render output path is required')
        return
      }
      if (expectedPath !== null && renderSettingsDraft.outputPath !== expectedPath) {
        setPendingOverwritePath(null)
        return
      }
      try {
        await execution.submit({
          settings: renderSettingsDraft,
          batchReview,
          overwrite,
          onAccepted: () => {
            clearOutputReview()
            if (target === 'current') setRenderDialogPhase('progress')
          },
        })
      } catch (error) {
        if (target === 'batch' && error.code === 'reinspectionRequired') throw error
        else if (target === 'current' && error.code === 'already_exists') setPendingOverwritePath(renderSettingsDraft.outputPath)
        else if (target === 'current' && error.code === 'output_error') setOutputPathError(error.message)
        else {
          if (target === 'current') setRenderDialogPhase('closed')
          setErrorMessage(error.message)
        }
      }
    },
    [backendStatus, clearOutputReview, execution, renderSettingsDraft, setErrorMessage],
  )

  const handleRenderVideoConfirm = useCallback((batchReview) => submitRender(batchReview), [submitRender])
  const handleOverwriteConfirm = useCallback(() => {
    if (pendingOverwritePath !== null) return submitRender(null, true, pendingOverwritePath)
  }, [pendingOverwritePath, submitRender])
  const handleOverwriteCancel = useCallback(() => setPendingOverwritePath(null), [])
  const handleRenderPreviewFrame = useCallback(() => {
    if (!renderDisabled) return execution.renderPreviewFrame()
  }, [execution, renderDisabled])
  return {
    cancelRender: execution.cancel,
    closeRenderDialog,
    handleRenderPreviewFrame,
    handleRenderVideoConfirm,
    handleOverwriteCancel,
    handleOverwriteConfirm,
    openRenderDialog,
    renderDialogPhase,
    renderDisabled,
    renderPreviewFrameDisabled: renderDisabled,
    renderSettingsDraft,
    renderTooltipContent,
    renderingVideo,
    outputPathError,
    overwriteOpen: pendingOverwritePath !== null,
    pendingOverwritePath,
    updateRenderSettingsDraft,
  }
}
