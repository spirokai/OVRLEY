/**
 * Renders the app header portion of the application interface.
 * Pure presentational component — all data flows through grouped props.
 */

import ActivitySection from './ActivitySection'
import EditorToolbar from './EditorToolbar'
import ActionButtons from './ActionButtons'

/**
 * Renders the app header component.
 *
 * @param {object} props - Component props.
 * @param {*} props.activityControls - Activity control state and handlers.
 * @param {*} props.backendStatus - Current backend status.
 * @param {*} props.editorControls - Editor control state and handlers.
 * @param {function} props.onOpenDownloads - Callback invoked to open downloads.
 * @param {*} props.renderControls - Render control state and handlers.
 * @param {*} props.templateControls - Template control state and handlers.
 * @param {*} props.videoControls - Video import control state and handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function AppHeader({
  activityControls,
  backendStatus,
  editorControls,
  onOpenDownloads,
  renderControls,
  templateControls,
  videoControls,
}) {
  const { activityLabel, onOpenActivityFile } = activityControls
  const {
    backgroundMode,
    gridVisible,
    onResetZoom,
    onSetBackgroundMode,
    onSetGridVisible,
    onSetSnapToGrid,
    onZoomIn,
    onZoomOut,
    snapToGrid,
    zoomLevel,
  } = editorControls
  const { onOpenRenderDialog, renderDisabled, renderTooltipContent, renderingVideo } = renderControls
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
  const { importedVideoFilename, handleImportVideo, clearImportedVideo } = videoControls

  return (
    <header className="relative z-50 shrink-0 border-b border-border/70 bg-card backdrop-blur-sm">
      <div className="grid grid-cols-[55%_auto_minmax(12rem,1fr)] items-center gap-6 px-6 py-3">
        <ActivitySection
          activityLabel={activityLabel}
          onOpenActivityFile={onOpenActivityFile}
          importedVideoFilename={importedVideoFilename}
          handleImportVideo={handleImportVideo}
          clearImportedVideo={clearImportedVideo}
          loadedTemplateSource={loadedTemplateSource}
          loadedTemplateFilename={loadedTemplateFilename}
          handleTemplateChange={handleTemplateChange}
          templates={templates}
          config={config}
          showTemplateStatus={showTemplateStatus}
          handleCreateNewTemplate={handleCreateNewTemplate}
          handleSaveTemplate={handleSaveTemplate}
          handleImportTemplate={handleImportTemplate}
        />

        <EditorToolbar
          backgroundMode={backgroundMode}
          onSetBackgroundMode={onSetBackgroundMode}
          importedVideoFilename={importedVideoFilename}
          zoomLevel={zoomLevel}
          onZoomIn={onZoomIn}
          onZoomOut={onZoomOut}
          onResetZoom={onResetZoom}
          gridVisible={gridVisible}
          onSetGridVisible={onSetGridVisible}
          snapToGrid={snapToGrid}
          onSetSnapToGrid={onSetSnapToGrid}
        />

        <ActionButtons
          onOpenRenderDialog={onOpenRenderDialog}
          renderDisabled={renderDisabled}
          renderTooltipContent={renderTooltipContent}
          renderingVideo={renderingVideo}
          backendStatus={backendStatus}
          onOpenDownloads={onOpenDownloads}
        />
      </div>
    </header>
  )
}
