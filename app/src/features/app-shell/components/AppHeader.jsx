/**
 * Renders the app header portion of the application interface.
 * Pure presentational component — all data flows through grouped props.
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
 * @param {*} props.activityControls - Activity control state and handlers.
 * @param {*} props.backendStatus - Current backend status.
 * @param {*} props.keyboardShortcutsControls - Keyboard shortcuts dialog state and handlers.
 * @param {function} props.onOpenDownloads - Callback invoked to open downloads.
 * @param {*} props.renderControls - Render control state and handlers.
 * @param {*} props.templateControls - Template control state and handlers.
 * @param {*} props.videoControls - Video import control state and handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function AppHeader({
  activityControls,
  backendStatus,
  keyboardShortcutsControls,
  onOpenDownloads,
  renderControls,
  templateControls,
  videoControls,
}) {
  const rawAppVersion = import.meta.env.VITE_OVRLEY_VERSION?.trim() || '0.00.0'
  const appVersion = rawAppVersion.startsWith('v') ? rawAppVersion : `v${rawAppVersion}`
  const { activityLabel, onOpenActivityFile } = activityControls
  const { onOpenRenderDialog, onRenderPreviewFrame, renderDisabled, renderPreviewFrameDisabled, renderTooltipContent, renderingVideo } =
    renderControls
  const {
    config,
    handleCreateNewTemplate,
    handleImportTemplate,
    handleSaveTemplate,
    handleTemplateChange,
    loadedTemplateFilename,
    loadedTemplateSource,
    onOpenChange,
    open,
    showTemplateStatus,
    templates,
  } = templateControls
  const { debugModeEnabled, importedMediaFilename, handleImportVideo, clearImportedVideo } = videoControls

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
    <header className="relative z-50 shrink-0 select-none border-b border-border/70 bg-card backdrop-blur-sm" onMouseDown={handleHeaderMouseDown}>
      <div className="grid grid-cols-[auto_auto_minmax(0,1fr)] items-center gap-x-6 pb-3 pl-6 pr-1 pt-3">
        <ActivitySection
          activityLabel={activityLabel}
          onOpenActivityFile={onOpenActivityFile}
          debugModeEnabled={debugModeEnabled}
          appVersion={appVersion}
          importedMediaFilename={importedMediaFilename}
          handleImportVideo={handleImportVideo}
          clearImportedVideo={clearImportedVideo}
        />

        <TemplateSection
          loadedTemplateSource={loadedTemplateSource}
          loadedTemplateFilename={loadedTemplateFilename}
          handleTemplateChange={handleTemplateChange}
          templates={templates}
          config={config}
          showTemplateStatus={showTemplateStatus}
          handleCreateNewTemplate={handleCreateNewTemplate}
          handleSaveTemplate={handleSaveTemplate}
          handleImportTemplate={handleImportTemplate}
          open={open}
          onOpenChange={onOpenChange}
          className="ml-4"
        />

        <ActionButtons
          onOpenKeyboardShortcuts={keyboardShortcutsControls.openKeyboardShortcuts}
          onOpenRenderDialog={onOpenRenderDialog}
          onRenderPreviewFrame={onRenderPreviewFrame}
          renderDisabled={renderDisabled}
          renderPreviewFrameDisabled={renderPreviewFrameDisabled}
          renderTooltipContent={renderTooltipContent}
          renderingVideo={renderingVideo}
          backendStatus={backendStatus}
          onOpenDownloads={onOpenDownloads}
        />
      </div>
    </header>
  )
}
