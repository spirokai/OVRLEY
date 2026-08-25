/**
 * Renders the app header portion of the application interface.
 * Pure presentational component consuming canonical domain objects.
 */

import ActivitySection from './ActivitySection'
import ActionButtons from './ActionButtons'
import TemplateSection from './TemplateSection'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isInteractiveElement } from '@/lib/utils'

/**
 * Renders the app header component.
 *
 * @param {object} props - Component props.
 * @param {*} props.activityImport - Activity import state and handlers.
 * @param {*} props.appShell - App shell state.
 * @param {*} props.backendState - Current backend state.
 * @param {*} props.editorShell - Editor shell state and actions.
 * @param {function} props.onOpenOutputDirectory - Callback invoked to open render output.
 * @param {*} props.renderWorkflow - Render workflow state and actions.
 * @param {*} props.templateManagement - Template state and actions.
 * @param {*} props.videoControls - Video import control state and handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function AppHeader({
  activityImport,
  appShell,
  backendState,
  editorShell,
  onOpenOutputDirectory,
  renderWorkflow,
  templateManagement,
  videoControls,
}) {
  const rawAppVersion = import.meta.env.VITE_OVRLEY_VERSION?.trim() || '0.00.0'
  const appVersion = rawAppVersion.startsWith('v') ? rawAppVersion : `v${rawAppVersion}`

  const handleHeaderMouseDown = (event) => {
    if (event.button !== 0 || event.defaultPrevented || isInteractiveElement(event.target)) {
      return
    }

    try {
      getCurrentWindow()
        .startDragging()
        .catch(() => {})
    } catch {
      return
    }
  }

  return (
    <header className="relative z-50 shrink-0 select-none border-b border-border bg-card backdrop-blur-sm" onMouseDown={handleHeaderMouseDown}>
      <div className="grid grid-cols-[auto_auto_minmax(0,1fr)] items-center gap-x-6 pb-3 pl-6 pr-1 pt-3">
        <ActivitySection
          activityFilename={activityImport.activityFilename}
          onOpenActivityFile={activityImport.handleActivityFileOpen}
          debugModeEnabled={editorShell.debugModeEnabled}
          appVersion={appVersion}
          importedMediaFilename={videoControls.importedMediaFilename}
          handleImportVideo={videoControls.handleImportVideo}
          clearImportedVideo={videoControls.clearImportedVideo}
        />

        <TemplateSection
          loadedTemplateSource={templateManagement.loadedTemplateSource}
          loadedTemplateFilename={templateManagement.loadedTemplateFilename}
          handleTemplateChange={templateManagement.handleTemplateChange}
          templates={templateManagement.templates}
          config={appShell.config}
          showTemplateStatus={templateManagement.showTemplateStatus}
          handleCreateNewTemplate={templateManagement.handleCreateNewTemplate}
          handleSaveTemplate={templateManagement.handleSaveTemplate}
          handleImportTemplate={templateManagement.handleImportTemplate}
          open={templateManagement.templateSelectorOpen}
          onOpenChange={templateManagement.setTemplateSelectorOpen}
          className="ml-4"
        />

        <ActionButtons
          onOpenKeyboardShortcuts={editorShell.openKeyboardShortcuts}
          onOpenRenderDialog={renderWorkflow.openRenderDialog}
          onRenderPreviewFrame={editorShell.debugModeEnabled ? renderWorkflow.handleRenderPreviewFrame : undefined}
          renderDisabled={renderWorkflow.renderDisabled}
          renderPreviewFrameDisabled={editorShell.debugModeEnabled ? renderWorkflow.renderPreviewFrameDisabled : undefined}
          renderTooltipContent={renderWorkflow.renderTooltipContent}
          renderingVideo={renderWorkflow.renderingVideo}
          backendStatus={backendState.backendStatus}
          onOpenOutputDirectory={onOpenOutputDirectory}
        />
      </div>
    </header>
  )
}
