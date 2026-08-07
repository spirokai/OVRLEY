/**
 * Composes shell-level hooks and returns their owned state unchanged.
 */

import { useEffect } from 'react'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { useAppShellStore } from '@/hooks/useAppStoreSelectors'
import { useRenderWorkflow } from '@/features/render-video'
import { useTemplateManagement } from '@/features/template-manager'
import useActivityImport from './useActivityImport'
import useAppBootstrap from './useAppBootstrap'
import useAppShellKeyboard from './useAppShellKeyboard'
import useBackendStatus from './useBackendStatus'
import useEditorShellState from './useEditorShellState'
import { useAppUpdate } from '@/features/app-update'
import { useVideoImport } from '@/features/video-preview'
import { useUndoRedo } from '@/features/undo-redo'
import * as backend from '@/api/backend'

/**
 * Orchestrates all shell-level hooks without adapting their public APIs.
 *
 * @returns {{
 *   activityImport: object,
 *   appUpdate: object,
 *   appShell: object,
 *   backendState: object,
 *   editorShell: object,
 *   handleOpenDownloads: Function,
 *   renderWorkflow: object,
 *   templateManagement: object,
 *   undoRedoControls: object,
 *   videoControls: object,
 *   widgetLiveEdits: object,
 * }}
 */
export default function useAppShellComposition() {
  const appShell = useAppShellStore()
  const widgetLiveEdits = useWidgetDraftState()
  const backendState = useBackendStatus()
  const editorShell = useEditorShellState()
  const activityImport = useActivityImport()
  const templateManagement = useTemplateManagement({ onTemplateCreated: editorShell.resetZoom })
  const renderWorkflow = useRenderWorkflow({ backendStatus: backendState.backendStatus })
  const videoControls = useVideoImport({ debugModeEnabled: editorShell.debugModeEnabled, onSetBackgroundMode: editorShell.setEditorBackgroundMode })
  const undoRedoControls = useUndoRedo({
    disabled: renderWorkflow.renderDialogPhase !== 'closed' || templateManagement.showNewTemplateConfirm,
  })

  useAppBootstrap()
  const appUpdate = useAppUpdate()

  const { restoreLastLoadedTemplate } = templateManagement

  useEffect(() => {
    restoreLastLoadedTemplate()
  }, [restoreLastLoadedTemplate])

  const handleOpenDownloads = async () => {
    try {
      await backend.openDownloads()
    } catch (error) {
      console.error('Error opening downloads:', error)
      appShell.setErrorMessage(`Failed to open downloads folder: ${error.message}`)
    }
  }

  useAppShellKeyboard({
    activityImport,
    appShell,
    backendState,
    handleOpenDownloads,
    renderWorkflow,
    templateManagement,
    videoControls,
  })

  return {
    activityImport,
    appUpdate,
    appShell,
    backendState,
    editorShell,
    handleOpenDownloads,
    renderWorkflow,
    templateManagement,
    undoRedoControls,
    videoControls,
    widgetLiveEdits,
  }
}
