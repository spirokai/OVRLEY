/**
 * Composes shell-level hooks and returns their owned state unchanged.
 */

import { useEffect } from 'react'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { useAppShellStore, useLayoutStore } from '@/hooks/useAppStoreSelectors'
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
import { loadRememberedRenderDirectory } from '@/features/render-video/utils/render-output'

/**
 * Orchestrates all shell-level hooks without adapting their public APIs.
 *
 * @returns {{
 *   activityImport: object,
 *   appUpdate: object,
 *   appShell: object,
 *   backendState: object,
 *   editorShell: object,
 *   handleOpenOutputDirectory: Function,
 *   layout: object,
 *   renderWorkflow: object,
 *   templateManagement: object,
 *   undoRedoControls: object,
 *   videoControls: object,
 *   widgetLiveEdits: object,
 * }}
 */
export default function useAppShellComposition() {
  const appShell = useAppShellStore()
  const layout = useLayoutStore()
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

  const handleOpenOutputDirectory = async () => {
    try {
      const rememberedDirectory = await loadRememberedRenderDirectory()
      await backend.openOutputDirectory(rememberedDirectory)
    } catch (error) {
      console.error('Error opening render output directory:', error)
      appShell.setErrorMessage(`Failed to open render output folder: ${error.message}`)
    }
  }

  useAppShellKeyboard({
    activityImport,
    appShell,
    backendState,
    handleOpenOutputDirectory,
    renderWorkflow,
    templateManagement,
    videoControls,
    layout,
  })

  return {
    activityImport,
    appUpdate,
    appShell,
    backendState,
    editorShell,
    handleOpenOutputDirectory,
    layout,
    renderWorkflow,
    templateManagement,
    undoRedoControls,
    videoControls,
    widgetLiveEdits,
  }
}
