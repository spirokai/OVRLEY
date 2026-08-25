/**
 * Composes the main application shell for the OVRLEY overlay editor.
 *
 * The useAppShellComposition hook owns shell-level hooks and returns their
 * canonical domain objects to the render tree.
 */

import { useEffect } from 'react'
import { OverlayEditor } from '@/features/overlay-editor'
import { OverlayPlayer } from '@/features/player'
import { RenderVideoDialog } from '@/features/render-video'
import { WidgetDrawerContent } from '@/features/widget-drawer'
import { ToolbarDrawerLayout, useToolbarDrawer } from '@/features/toolbar'
import { WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'
import { NewTemplateConfirmDialog } from '@/features/template-manager'
import { UpdatePromptDialog } from '@/features/app-update'
import { AppHeader, ControlPanel, ErrorAlert, KeyboardShortcutsDialog, LoadingOverlay, useAppShellComposition } from '@/features/app-shell'
import * as backend from './api/backend'

function useRightClickDevtools() {
  useEffect(() => {
    if (!backend.hasTauriRuntime()) {
      return undefined
    }

    const handleContextMenu = (event) => {
      event.preventDefault()
      import('@tauri-apps/api/core')
        .then(({ invoke }) => invoke('plugin:webview|internal_toggle_devtools'))
        .catch((error) => {
          console.error('Failed to toggle DevTools:', error)
        })
    }

    window.addEventListener('contextmenu', handleContextMenu)
    return () => {
      window.removeEventListener('contextmenu', handleContextMenu)
    }
  }, [])
}

/**
 * Renders the main application shell.
 * @returns {JSX.Element} Rendered component output.
 */
function AppShell() {
  useRightClickDevtools()

  const {
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
  } = useAppShellComposition()
  const toolbarDrawer = useToolbarDrawer(layout)
  const { config, globalDefaults, importingVideo, isProcessing, setConfig } = appShell

  if (!toolbarDrawer.initialized) {
    return (
      <div className="flex h-screen w-full items-center justify-center bg-background text-foreground">
        <span className="text-sm text-muted-foreground">OVRLEY is starting...</span>
      </div>
    )
  }

  return (
    <div
      className="app-shell"
      style={{
        '--app-scale': `${editorShell.uiScale}`,
      }}
    >
      <div className="relative flex h-full flex-col bg-background text-foreground">
        <ErrorAlert />
        <UpdatePromptDialog {...appUpdate} />
        <RenderVideoDialog
          phase={renderWorkflow.renderDialogPhase}
          settings={renderWorkflow.renderSettingsDraft}
          onSettingsChange={renderWorkflow.updateRenderSettingsDraft}
          onClose={renderWorkflow.closeRenderDialog}
          onConfirm={renderWorkflow.handleRenderVideoConfirm}
          onOutputPathChange={renderWorkflow.handleOutputPathChange}
          outputPathError={renderWorkflow.outputPathError}
          overwriteOpen={renderWorkflow.overwriteOpen}
          pendingOverwritePath={renderWorkflow.pendingOverwritePath}
          onOverwriteConfirm={renderWorkflow.handleOverwriteConfirm}
          onOverwriteCancel={renderWorkflow.handleOverwriteCancel}
          submissionPending={renderWorkflow.submissionPending}
        />
        <NewTemplateConfirmDialog
          open={templateManagement.showNewTemplateConfirm}
          onCancel={() => templateManagement.setShowNewTemplateConfirm(false)}
          onConfirm={templateManagement.confirmCreateNewTemplate}
        />
        <KeyboardShortcutsDialog open={editorShell.keyboardShortcutsOpen} onClose={editorShell.closeKeyboardShortcuts} />
        <AppHeader
          activityImport={activityImport}
          appShell={appShell}
          backendState={backendState}
          editorShell={editorShell}
          onOpenOutputDirectory={handleOpenOutputDirectory}
          renderWorkflow={renderWorkflow}
          templateManagement={templateManagement}
          videoControls={videoControls}
        />

        <ToolbarDrawerLayout
          {...toolbarDrawer}
          drawerContent={
            toolbarDrawer.renderDrawerContent && toolbarDrawer.activeTool === WIDGETS_TOOL ? (
              <WidgetDrawerContent widgetLiveEdits={widgetLiveEdits} />
            ) : null
          }
          workspace={
            <>
              <LoadingOverlay
                show={isProcessing || importingVideo}
                label={importingVideo ? 'Importing your video...' : 'Processing your activity...'}
              />
              <div
                className="min-h-0 flex-1"
                onFocusCapture={() => editorShell.setActiveKeyboardWorkspace('editor')}
                onPointerDownCapture={() => editorShell.setActiveKeyboardWorkspace('editor')}
              >
                <OverlayEditor
                  config={config}
                  globalDefaults={globalDefaults}
                  onConfigChange={setConfig}
                  zoomLevel={editorShell.editorZoomLevel}
                  onZoomLevelChange={editorShell.setEditorZoomLevel}
                  backgroundMode={editorShell.editorBackgroundMode}
                  gridVisible={editorShell.editorGridVisible}
                  snapToGrid={editorShell.editorSnapToGrid}
                  importedBackgroundImageFilename={videoControls.importedBackgroundImageFilename}
                  importedVideoFilename={videoControls.importedVideoFilename}
                  editorShell={editorShell}
                  undoRedoControls={undoRedoControls}
                  showTemplateStatus={templateManagement.showTemplateStatus}
                  templateStatus={templateManagement.status}
                  widgetLiveEdits={widgetLiveEdits}
                />
              </div>
              <OverlayPlayer
                activeKeyboardWorkspace={editorShell.activeKeyboardWorkspace}
                backgroundMode={editorShell.editorBackgroundMode}
                onActivateKeyboardWorkspace={() => editorShell.setActiveKeyboardWorkspace('player')}
              />
            </>
          }
          controlPanel={
            <div
              className="w-106 min-w-106 max-w-106 shrink-0 overflow-y-auto border-l border-border bg-card/60 backdrop-blur-sm"
              onFocusCapture={() => editorShell.setActiveKeyboardWorkspace('editor')}
              onPointerDownCapture={() => editorShell.setActiveKeyboardWorkspace('editor')}
            >
              <ControlPanel config={config} onConfigChange={setConfig} widgetLiveEdits={widgetLiveEdits} />
            </div>
          }
        />
      </div>
    </div>
  )
}

/**
 * Renders the top-level application shell.
 * @returns {JSX.Element} Rendered component output.
 */
function App() {
  return <AppShell />
}

export default App
