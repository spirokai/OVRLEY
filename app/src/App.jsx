/**
 * Composes the main application shell for the OVRLEY overlay editor.
 *
 * The useAppShellComposition hook orchestrates all shell-level hooks and
 * returns domain-grouped objects consumed by AppHeader and child components.
 * This keeps the render tree clean while avoiding hand-rolled grouping in
 * the JSX body.
 */

import { OverlayEditor } from '@/features/overlay-editor'
import { OverlayPlayer } from '@/features/player'
import { RenderVideoDialog } from '@/features/render-video'
import { WidgetDrawer } from '@/features/widget-drawer'
import { useAppShellStore } from '@/hooks/useAppStoreSelectors'
import { useRenderWorkflow } from '@/features/render-video'
import { NewTemplateConfirmDialog, useTemplateManagement } from '@/features/template-manager'
import {
  AppHeader,
  ControlPanel,
  ErrorAlert,
  LoadingOverlay,
  TitleBar,
  useActivityImport,
  useAppBootstrap,
  useBackendStatus,
  useEditorShellState,
} from '@/features/app-shell'
import { useVideoImport } from '@/features/video-preview'
import { WasmPreviewDebug } from '@/features/wasm-preview-debug'
import * as backend from './api/backend'

/**
 * Orchestrates all shell-level hooks and returns domain-grouped objects.
 *
 * Each group is owned by the hook that produces the data (e.g. editorControls
 * from useEditorShellState, renderControls from useRenderWorkflow). The JSX
 * render tree destructures these groups — no hand-rolled intermediate objects
 * in the component body.
 *
 * @returns {{
 *   activityControls: { activityLabel: string, onOpenActivityFile: Function },
 *   backendStatus: string,
 *   config: object,
 *   editorControls: object,
 *   editorShell: object,
 *   globalDefaults: object,
 *   handleOpenDownloads: Function,
 *   importingVideo: boolean,
 *   isProcessing: boolean,
 *   renderControls: object,
 *   renderWorkflow: object,
 *   setConfig: Function,
 *   templateControls: object,
 *   templateManagement: object,
 *   videoControls: object,
 * }}
 */
function useAppShellComposition() {
  const { config, isProcessing, globalDefaults, importingVideo, setConfig, setErrorMessage } = useAppShellStore()
  const { backendStatus } = useBackendStatus()
  const editorShell = useEditorShellState()
  const { activityFilename, handleGpxFileOpen } = useActivityImport()
  const templateManagement = useTemplateManagement({ onTemplateCreated: editorShell.resetZoom })
  const renderWorkflow = useRenderWorkflow({ backendStatus })
  const videoControls = useVideoImport({ debugModeEnabled: editorShell.debugModeEnabled, onSetBackgroundMode: editorShell.setEditorBackgroundMode })

  useAppBootstrap()

  const handleOpenDownloads = async () => {
    try {
      await backend.openDownloads()
    } catch (error) {
      console.error('Error opening downloads:', error)
      setErrorMessage(`Failed to open downloads folder: ${error.message}`)
    }
  }

  const activityControls = {
    activityLabel: activityFilename === 'demo.gpxinit' ? 'Load GPX/FIT/SRT' : activityFilename || 'Load GPX/FIT/SRT',
    onOpenActivityFile: handleGpxFileOpen,
  }

  const editorControls = {
    backgroundMode: editorShell.editorBackgroundMode,
    gridVisible: editorShell.editorGridVisible,
    onResetZoom: editorShell.resetZoom,
    onSetBackgroundMode: editorShell.setEditorBackgroundMode,
    onSetGridVisible: editorShell.setEditorGridVisible,
    onSetSnapToGrid: editorShell.setEditorSnapToGrid,
    onZoomIn: editorShell.increaseZoom,
    onZoomOut: editorShell.decreaseZoom,
    snapToGrid: editorShell.editorSnapToGrid,
    zoomLevel: editorShell.editorZoomLevel,
  }

  const renderControls = {
    onOpenRenderDialog: renderWorkflow.openRenderDialog,
    onRenderPreviewFrame: editorShell.debugModeEnabled ? renderWorkflow.handleRenderPreviewFrame : undefined,
    renderPreviewFrameDisabled: editorShell.debugModeEnabled ? renderWorkflow.renderPreviewFrameDisabled : undefined,
    renderDisabled: renderWorkflow.renderDisabled,
    renderTooltipContent: renderWorkflow.renderTooltipContent,
    renderingVideo: renderWorkflow.renderingVideo,
  }

  const templateControls = {
    config,
    handleCreateNewTemplate: templateManagement.handleCreateNewTemplate,
    handleImportTemplate: templateManagement.handleImportTemplate,
    handleSaveTemplate: templateManagement.handleSaveTemplate,
    handleTemplateChange: templateManagement.handleTemplateChange,
    loadedTemplateFilename: templateManagement.loadedTemplateFilename,
    loadedTemplateSource: templateManagement.loadedTemplateSource,
    showTemplateStatus: templateManagement.showTemplateStatus,
    status: templateManagement.status,
    templates: templateManagement.templates,
  }

  return {
    activityControls,
    backendStatus,
    config,
    editorControls,
    editorShell,
    globalDefaults,
    handleOpenDownloads,
    importingVideo,
    isProcessing,
    renderControls,
    renderWorkflow,
    setConfig,
    templateControls,
    templateManagement,
    videoControls,
  }
}

/**
 * Renders the main application shell.
 * @returns {JSX.Element} Rendered component output.
 */
function AppShell() {
  const {
    activityControls,
    backendStatus,
    config,
    editorControls,
    editorShell,
    globalDefaults,
    handleOpenDownloads,
    importingVideo,
    isProcessing,
    renderControls,
    renderWorkflow,
    setConfig,
    templateControls,
    templateManagement,
    videoControls,
  } = useAppShellComposition()

  return (
    <div
      className="app-shell"
      style={{
        '--app-scale': `${editorShell.uiScale}`,
      }}
    >
      <div className="relative flex h-full flex-col bg-background text-foreground">
        <ErrorAlert />
        <RenderVideoDialog
          phase={renderWorkflow.renderDialogPhase}
          settings={renderWorkflow.renderSettingsDraft}
          onSettingsChange={renderWorkflow.updateRenderSettingsDraft}
          onClose={renderWorkflow.closeRenderDialog}
          onConfirm={renderWorkflow.handleRenderVideoConfirm}
        />
        <NewTemplateConfirmDialog
          open={templateManagement.showNewTemplateConfirm}
          onCancel={() => templateManagement.setShowNewTemplateConfirm(false)}
          onConfirm={templateManagement.confirmCreateNewTemplate}
        />
        <TitleBar />
        <AppHeader
          activityControls={activityControls}
          backendStatus={backendStatus}
          editorControls={editorControls}
          onOpenDownloads={handleOpenDownloads}
          renderControls={renderControls}
          templateControls={templateControls}
          videoControls={videoControls}
        />

        <div className="flex min-h-0 flex-1 overflow-hidden">
          <div className="relative flex min-w-0 flex-1 flex-col bg-surface-darken">
            <LoadingOverlay show={isProcessing || importingVideo} label={importingVideo ? 'Importing media...' : 'Processing...'} />
            <WidgetDrawer />
            <div className="min-h-0 flex-1">
              <OverlayEditor
                config={config}
                globalDefaults={globalDefaults}
                onConfigChange={setConfig}
                zoomLevel={editorShell.editorZoomLevel}
                onZoomLevelChange={editorShell.setEditorZoomLevel}
                backgroundMode={editorShell.editorBackgroundMode}
                gridVisible={editorShell.editorGridVisible}
                snapToGrid={editorShell.editorSnapToGrid}
                showTemplateStatus={templateManagement.showTemplateStatus}
                templateStatus={templateManagement.status}
              />
            </div>
            <OverlayPlayer backgroundMode={editorShell.editorBackgroundMode} />
          </div>

          <div className="w-106 min-w-106 max-w-106 shrink-0 overflow-y-auto border-l border-border/70 bg-card/60 backdrop-blur-sm">
            <ControlPanel config={config} onConfigChange={setConfig} />
          </div>
        </div>
      </div>
    </div>
  )
}

/**
 * Renders the top-level application shell.
 * @returns {JSX.Element} Rendered component output.
 */
function App() {
  if (import.meta.env.DEV && window.location.hash === '#/debug/wasm-preview') {
    return <WasmPreviewDebug />
  }

  return <AppShell />
}

export default App
