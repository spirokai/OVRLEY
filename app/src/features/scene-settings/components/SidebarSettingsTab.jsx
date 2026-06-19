/**
 * Renders the sidebar settings tab by composing section components
 * with state and handlers from the container hook.
 *
 * The hook now returns grouped objects (overlaySettings, videoSyncSettings,
 * globalSettings, handlers) so each section receives a coherent block instead
 * of 40+ individually destructured keys.
 *
 * @param {object} props - Component props.
 * @param {object} props.config - Overlay template configuration data.
 * @param {function} props.onConfigChange - Callback invoked on config change.
 * @returns {JSX.Element} Rendered component output.
 */

import useSceneSettingsState from '../hooks/useSceneSettingsState'
import OverlaySettingsSection from './OverlaySettingsSection'
import VideoSyncSection from './VideoSyncSection'
import GlobalSettingsSection from './GlobalSettingsSection'

export default function SidebarSettingsTab({ config, onConfigChange }) {
  const state = useSceneSettingsState({ config, onConfigChange })
  const { overlaySettings, videoSyncSettings, globalSettings, handlers } = state

  return (
    <div className="space-y-8 outline-none pb-6 px-4">
      <div className="space-y-4">
        <OverlaySettingsSection
          aspectRatio={overlaySettings.aspectRatio}
          onAspectRatioChange={handlers.handleAspectRatioChange}
          resId={overlaySettings.resId}
          onResChange={handlers.handleResolutionChange}
          scene={overlaySettings.scene}
          onUpdateScene={handlers.updateScene}
          importedVideoFps={overlaySettings.importedVideoFps}
          fpsMode={overlaySettings.fpsMode}
          onFpsModeChange={handlers.handleFpsModeChange}
          onCustomFpsChange={handlers.handleCustomFpsChange}
          updateRate={overlaySettings.updateRate}
          updateRateOptions={overlaySettings.updateRateOptions}
          onUpdateRateChange={handlers.handleUpdateRateChange}
          activitySummary={overlaySettings.activitySummary}
          importedVideoPath={overlaySettings.importedVideoPath}
          exportRange={overlaySettings.exportRange}
          onExportRangeChange={state.setExportRange}
        />

        {videoSyncSettings.importedVideoPath ? (
          <VideoSyncSection
            importedVideoDuration={videoSyncSettings.importedVideoDuration}
            importedVideoFps={videoSyncSettings.importedVideoFps}
            importedVideoResolution={videoSyncSettings.importedVideoResolution}
            importedVideoCreationTime={videoSyncSettings.importedVideoCreationTime}
            videoSyncWarning={videoSyncSettings.videoSyncWarning}
            videoResolutionMismatch={videoSyncSettings.videoResolutionMismatch}
            offsetInput={videoSyncSettings.offsetInput}
            onOffsetInputChange={videoSyncSettings.setOffsetInput}
            onOffsetBlur={handlers.handleOffsetBlur}
            onIncrement={handlers.handleIncrement}
            activitySummary={videoSyncSettings.activitySummary}
            onComputeVideoSync={videoSyncSettings.computeVideoSync}
          />
        ) : null}
      </div>

      <GlobalSettingsSection
        globalDefaults={globalSettings.globalDefaults}
        onGlobalDefaultChange={globalSettings.setGlobalDefault}
        onResetDefaults={globalSettings.resetGlobalDefaults}
        sceneStyleValue={globalSettings.sceneStyleValue}
        availableFonts={globalSettings.availableFonts}
      />
    </div>
  )
}
