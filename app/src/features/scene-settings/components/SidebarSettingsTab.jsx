/**
 * Renders the sidebar settings tab by composing section components
 * with state and handlers from the container hook.
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

  return (
    <div className="mt-4 space-y-8 outline-none pb-10">
      <div className="space-y-4">
        <OverlaySettingsSection
          aspectRatio={state.aspectRatio}
          onAspectRatioChange={state.handleAspectRatioChange}
          resId={state.resId}
          onResChange={state.handleResolutionChange}
          scene={state.scene}
          onUpdateScene={state.updateScene}
          importedVideoFps={state.importedVideoFps}
          fpsMode={state.fpsMode}
          onFpsModeChange={state.handleFpsModeChange}
          onCustomFpsChange={state.handleCustomFpsChange}
          updateRate={state.updateRate}
          updateRateOptions={state.updateRateOptions}
          onUpdateRateChange={state.handleUpdateRateChange}
          activitySummary={state.activitySummary}
          importedVideoPath={state.importedVideoPath}
          exportRange={state.exportRange}
          onExportRangeChange={state.setExportRange}
        />

        {state.importedVideoPath ? (
          <VideoSyncSection
            importedVideoDuration={state.importedVideoDuration}
            importedVideoFps={state.importedVideoFps}
            importedVideoResolution={state.importedVideoResolution}
            importedVideoCreationTime={state.importedVideoCreationTime}
            videoSyncWarning={state.videoSyncWarning}
            videoResolutionMismatch={state.videoResolutionMismatch}
            offsetInput={state.offsetInput}
            onOffsetInputChange={state.setOffsetInput}
            onOffsetBlur={state.handleOffsetBlur}
            onIncrement={state.handleIncrement}
            activitySummary={state.activitySummary}
            onComputeVideoSync={state.computeVideoSync}
          />
        ) : null}
      </div>

      <GlobalSettingsSection
        globalDefaults={state.globalDefaults}
        onGlobalDefaultChange={state.setGlobalDefault}
        onResetDefaults={state.resetGlobalDefaults}
        sceneStyleValue={state.sceneStyleValue}
        systemFonts={state.systemFonts}
      />
    </div>
  )
}
