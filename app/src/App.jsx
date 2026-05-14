/**
 * Composes the main application shell for the OVRLEY overlay editor.
 */

import AppHeader from '@/components/AppHeader'
import TitleBar from '@/components/TitleBar'
import ControlPanel from '@/components/ControlPanel'
import ErrorAlert from '@/components/ErrorAlert'
import LoadingOverlay from '@/components/LoadingOverlay'
import OverlayEditor from '@/components/OverlayEditor'
import { OverlayPlayer } from '@/features/player'
import { RenderVideoDialog } from '@/features/render-video'
import useActivityImport from '@/hooks/useActivityImport'
import useAppBootstrap from '@/hooks/useAppBootstrap'
import { useAppShellStore } from '@/hooks/useAppStoreSelectors'
import useBackendStatus from '@/hooks/useBackendStatus'
import useEditorShellState from '@/hooks/useEditorShellState'
import { useRenderWorkflow } from '@/features/render-video'
import { NewTemplateConfirmDialog, useTemplateManagement } from '@/features/template-manager'
import useVideoImport from '@/hooks/useVideoImport'
import './index.css'
import * as backend from './api/backend'

/**
 * Renders the main application shell.
 * @returns {JSX.Element} Rendered component output.
 */
function AppShell() {
  const { config, generatingImage, globalDefaults, setConfig, setErrorMessage } = useAppShellStore()
  const { backendStatus } = useBackendStatus()
  const editorShell = useEditorShellState()
  const { gpxFilename, handleGpxFileOpen } = useActivityImport()
  const templateManagement = useTemplateManagement({
    onTemplateCreated: editorShell.resetZoom,
  })
  const renderWorkflow = useRenderWorkflow({ backendStatus })
  const videoControls = useVideoImport()

  useAppBootstrap()

  const handleOpenDownloads = async () => {
    try {
      await backend.openDownloads()
    } catch (error) {
      console.error('Error opening downloads:', error)
      setErrorMessage(`Failed to open downloads folder: ${error.message}`)
    }
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
          activityControls={{
            activityLabel: gpxFilename === 'demo.gpxinit' ? 'Load GPX/FIT' : gpxFilename || 'Load GPX/FIT',
            onOpenActivityFile: handleGpxFileOpen,
          }}
          backendStatus={backendStatus}
          editorControls={{
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
          }}
          onOpenDownloads={handleOpenDownloads}
          renderControls={{
            onOpenRenderDialog: renderWorkflow.openRenderDialog,
            renderDisabled: renderWorkflow.renderDisabled,
            renderTooltipContent: renderWorkflow.renderTooltipContent,
            renderingVideo: renderWorkflow.renderingVideo,
          }}
          templateControls={{
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
          }}
          videoControls={videoControls}
        />

        <div className="flex min-h-0 flex-1 overflow-hidden">
          <div className="relative flex min-w-0 flex-1 flex-col bg-background">
            <LoadingOverlay show={generatingImage} />
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

          <div className="w-96 min-w-96 max-w-96 shrink-0 overflow-y-auto border-l border-border/70 bg-card/60 backdrop-blur-sm">
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
  return <AppShell />
}

export default App
