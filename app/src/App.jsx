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
import {
  ActivityDrawerContent,
  ProjectsDrawerContent,
  ToolbarDrawerLayout,
  useToolbarDrawer,
  VideoDrawerContent,
  useVideoSyncControls,
} from '@/features/toolbar'
import { ACTIVITY_TOOL, PROJECTS_TOOL, VIDEO_TOOL, WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'
import { MissingSourceDialog, StartupProjectsDialog } from '@/features/projects'
import { UpdatePromptDialog } from '@/features/app-update'
import {
  AppHeader,
  ControlPanel,
  ErrorAlert,
  KeyboardShortcutsDialog,
  LoadingOverlay,
  UnsavedChangesDialog,
  useAppShellComposition,
} from '@/features/app-shell'
import * as backend from './api/backend'
import { useTranslation } from 'react-i18next'

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
  const { t } = useTranslation()
  useRightClickDevtools()

  const {
    activityImport,
    appUpdate,
    appShell,
    backendState,
    editorShell,
    handleOpenOutputDirectory,
    layout,
    projectLifecycle,
    renderWorkflow,
    templateManagement,
    undoRedoControls,
    videoControls,
    widgetLiveEdits,
  } = useAppShellComposition()
  const toolbarDrawer = useToolbarDrawer(layout)
  const videoSync = useVideoSyncControls()
  const { config, globalDefaults, importingVideo, isProcessing, setConfig } = appShell
  let drawerContent = null

  if (toolbarDrawer.renderDrawerContent) {
    if (toolbarDrawer.activeTool === PROJECTS_TOOL) {
      drawerContent = (
        <ProjectsDrawerContent
          projectName={projectLifecycle.projectName}
          projectPath={projectLifecycle.loadedProjectPath}
          activityFilename={activityImport.activityFilename}
          activityPath={activityImport.activityPath}
          videoFilename={videoControls.videoSummary.filename}
          videoPath={videoControls.videoSummary.path}
          status={projectLifecycle.status}
          busy={projectLifecycle.busy}
          onNew={projectLifecycle.handleNewProject}
          onOpen={projectLifecycle.handleOpenProject}
          onSave={projectLifecycle.handleSaveProject}
          onSaveAs={projectLifecycle.handleSaveProjectAs}
        />
      )
    } else if (toolbarDrawer.activeTool === ACTIVITY_TOOL) {
      drawerContent = (
        <ActivityDrawerContent
          activitySummary={activityImport.activitySummary}
          filename={activityImport.activityFilename}
          onBrowseActivity={activityImport.handleActivityFileOpen}
          onDeleteActivity={activityImport.deleteActivity}
          onDropActivityFiles={activityImport.handleActivityFilesDrop}
        />
      )
    } else if (toolbarDrawer.activeTool === VIDEO_TOOL) {
      drawerContent = (
        <VideoDrawerContent
          videoSummary={videoControls.videoSummary}
          onBrowseVideo={videoControls.handleImportVideo}
          onDeleteVideo={videoControls.clearImportedVideo}
          onDropVideoFiles={videoControls.handleVideoFilesDrop}
          videoSync={videoSync}
        />
      )
    } else if (toolbarDrawer.activeTool === WIDGETS_TOOL) {
      drawerContent = <WidgetDrawerContent widgetLiveEdits={widgetLiveEdits} />
    }
  }

  if (!toolbarDrawer.initialized) {
    return (
      <div className="relative h-screen w-full bg-background text-foreground">
        <LoadingOverlay show label={t('app.ovrleyIsStarting', 'OVRLEY is starting...')} />
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
        <StartupProjectsDialog {...projectLifecycle.startupProjectDialog} open={projectLifecycle.startupProjectDialog.open && !appUpdate.open} />
        <UpdatePromptDialog {...appUpdate} />
        <RenderVideoDialog
          phase={renderWorkflow.renderDialogPhase}
          settings={renderWorkflow.renderSettingsDraft}
          onSettingsChange={renderWorkflow.updateRenderSettingsDraft}
          onClose={renderWorkflow.closeRenderDialog}
          onConfirm={renderWorkflow.handleRenderVideoConfirm}
          outputPathError={renderWorkflow.outputPathError}
          overwriteOpen={renderWorkflow.overwriteOpen}
          pendingOverwritePath={renderWorkflow.pendingOverwritePath}
          onOverwriteConfirm={renderWorkflow.handleOverwriteConfirm}
          onOverwriteCancel={renderWorkflow.handleOverwriteCancel}
          submissionPending={renderWorkflow.submissionPending}
        />
        <UnsavedChangesDialog {...templateManagement.newTemplateConfirmDialog} />
        <UnsavedChangesDialog {...projectLifecycle.unsavedProjectDialog} />
        <MissingSourceDialog {...projectLifecycle.missingSourceDialog} />
        <KeyboardShortcutsDialog open={editorShell.keyboardShortcutsOpen} onClose={editorShell.closeKeyboardShortcuts} />
        <AppHeader
          activityImport={activityImport}
          appShell={appShell}
          backendState={backendState}
          editorShell={editorShell}
          onOpenOutputDirectory={handleOpenOutputDirectory}
          projectLifecycle={projectLifecycle}
          renderWorkflow={renderWorkflow}
          templateManagement={templateManagement}
          videoControls={videoControls}
        />

        <ToolbarDrawerLayout
          {...toolbarDrawer}
          drawerContent={drawerContent}
          workspace={
            <>
              <LoadingOverlay
                show={projectLifecycle.loadingProject || isProcessing || importingVideo}
                label={
                  projectLifecycle.loadingProject
                    ? t('app.loadingProject', 'Loading project...')
                    : importingVideo
                      ? t('app.importingYourVideo', 'Importing your video...')
                      : t('app.processingYourActivity', 'Processing your activity...')
                }
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
                  showProjectStatus={projectLifecycle.status !== 'Saved'}
                  projectStatus={projectLifecycle.status}
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
