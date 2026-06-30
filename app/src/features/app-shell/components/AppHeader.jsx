/**
 * Renders the app header portion of the application interface.
 * Pure presentational component — all data flows through grouped props.
 */

import ActivitySection from './ActivitySection'
import ActionButtons from './ActionButtons'
import TemplateSection from './TemplateSection'

/**
 * Renders the app header component.
 *
 * @param {object} props - Component props.
 * @param {*} props.activityControls - Activity control state and handlers.
 * @param {*} props.backendStatus - Current backend status.
 * @param {function} props.onOpenDownloads - Callback invoked to open downloads.
 * @param {*} props.renderControls - Render control state and handlers.
 * @param {*} props.templateControls - Template control state and handlers.
 * @param {*} props.videoControls - Video import control state and handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function AppHeader({ activityControls, backendStatus, onOpenDownloads, renderControls, templateControls, videoControls }) {
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
    showTemplateStatus,
    templates,
  } = templateControls
  const { debugModeEnabled, importedMediaFilename, handleImportVideo, clearImportedVideo } = videoControls

  return (
    <header className="relative z-50 shrink-0 border-b border-border/70 bg-card backdrop-blur-sm">
      <div className="grid grid-cols-[auto_auto_minmax(0,1fr)] items-center gap-x-6 pb-3 pl-6 pr-1 pt-3">
        <ActivitySection
          activityLabel={activityLabel}
          onOpenActivityFile={onOpenActivityFile}
          debugModeEnabled={debugModeEnabled}
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
          className="ml-4"
        />
        <ActionButtons
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
