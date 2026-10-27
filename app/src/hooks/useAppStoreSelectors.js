import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { isRendererBusy } from '@/features/render-video/utils/renderRequest'
import { formatClockDuration } from '@/lib/time-format'
import { clamp } from '@/lib/utils'

function selectOverlayPreview(state) {
  return {
    exportRange: state.renderSettings.range,
    disabledLaps: state.disabledLaps,
    importedBackgroundImagePath: state.importedBackgroundImagePath,
    fallbackDurationSeconds: state.fallbackDurationSeconds,
  }
}

/** @returns {object} Source data and canonical timing for the overlay preview. The playhead is excluded so playback frames skip the editor. */
export function useOverlayPreviewStore() {
  return useStore(useShallow(selectOverlayPreview))
}

/**
 * Subscribes to the playhead only while the editor shows the video sync workspace, which renders it directly.
 * @param {boolean} enabled Whether video sync mode is active.
 * @returns {number|null} Store playhead in video sync mode, otherwise null.
 */
export function useVideoSyncPreviewSecond(enabled) {
  return useStore((state) => (enabled ? state.selectedSecond : null))
}

/**
 * Formats the clamped playhead for the player clock; the formatted string changes once per displayed second.
 * @param {number} timelineMinimum Earliest timeline second.
 * @param {number} totalDuration Timeline duration in seconds.
 * @returns {string} Formatted playhead clock.
 */
export function usePlayheadClockLabel(timelineMinimum, totalDuration) {
  return useStore((state) => formatClockDuration(clamp(state.selectedSecond, timelineMinimum, totalDuration)))
}

export function useLayoutStore() {
  return useStore(
    useShallow((state) => ({
      activeLeftDrawerTool: state.activeLeftDrawerTool,
      dismissLeftDrawerOverlay: state.dismissLeftDrawerOverlay,
      initializeLeftDrawer: state.initializeLeftDrawer,
      leftDrawerInitialized: state.leftDrawerInitialized,
      leftDrawerPinned: state.leftDrawerPinned,
      leftDrawerVisible: state.leftDrawerVisible,
      selectLeftDrawerTool: state.selectLeftDrawerTool,
      setLeftDrawerPinned: state.setLeftDrawerPinned,
    })),
  )
}

export function useAppShellStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      computeVideoSync: state.computeVideoSync,
      config: state.config,
      isProcessing: state.isProcessing,
      globalDefaults: state.globalDefaults,
      importingVideo: state.importingVideo,
      hasVideos: state.importedVideos.length > 0,
      setConfig: state.setConfig,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

export function useBootstrapStore() {
  return useStore(
    useShallow((state) => ({
      fetchAvailableCodecs: state.fetchAvailableCodecs,
      setPlatformOs: state.setPlatformOs,
    })),
  )
}

export function useActivityStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      activityFilename: state.activitySource?.path?.split(/[/\\]/).at(-1) ?? state.activitySummary?.fileName ?? null,
      activitySource: state.activitySource,
      clearActivityFile: state.clearActivityFile,
      disabledLaps: state.disabledLaps,
      hasExternalActivity: state.parsedActivity !== null,
      setLapEnabled: state.setLapEnabled,
      setErrorMessage: state.setErrorMessage,
      setProcessing: state.setProcessing,
    })),
  )
}

export function useTemplateStore() {
  return useStore(
    useShallow((state) => ({
      aspectRatio: state.aspectRatio,
      config: state.config,
      createNewTemplate: state.createNewTemplate,
      globalDefaults: state.globalDefaults,
      hydrateTemplateState: state.hydrateTemplateState,
      lastSavedTemplateState: state.lastSavedTemplateState,
      loadedTemplateSource: state.loadedTemplateSource,
      setErrorMessage: state.setErrorMessage,
      setProcessing: state.setProcessing,
      setLastSavedTemplateState: state.setLastSavedTemplateState,
      setLoadedTemplateSource: state.setLoadedTemplateSource,
      templates: state.templates,
      renderSettings: state.renderSettings,
    })),
  )
}

/** @returns {object} Shell availability and native session identities; frame progress stays in the dialog. */
export function useRenderStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      hasVideoTelemetry: state.importedVideos.some((video) => video.telemetry.status === 'ready'),
      activeRenderId: state.activeRenderId,
      activeBatchId: state.batchSnapshot?.rendererBusy ? state.batchSnapshot.batchId : null,
      config: state.config,
      rendererBusy: isRendererBusy(state),
      renderStatus: state.renderProgress.status,
      renderingVideo: state.renderingVideo,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

/** @returns {object|null} External activity used to initialize an editable export range. */
export function useRenderRangeActivity() {
  return useStore((state) => state.parsedActivity)
}

/** @returns {object} Renderer availability and inputs for dialog presentation. */
export function useRenderDialogInputs() {
  return useStore(
    useShallow((state) => ({
      availableCodecs: state.availableCodecs,
      config: state.config,
      importedVideos: state.importedVideos,
      videoEdits: state.videoEdits,
      platformOs: state.platformOs,
      rendererBusy: isRendererBusy(state),
      renderingVideo: state.renderingVideo,
      renderSubmissionTarget: state.renderSubmissionTarget,
      isCancelling: state.renderCancellationTarget !== null,
      renderProgress: state.renderProgress,
      batchSnapshot: state.batchSnapshot,
      renderSettings: state.renderSettings,
    })),
  )
}

/** @returns {object} Batch output choices, without native frame progress. */
export function useBatchRenderStore() {
  return useStore(
    useShallow((state) => ({
      batchOutputFolder: state.batchOutputFolder,
      batchSkipOverlay: state.batchSkipOverlay,
      hasBatchResults: state.batchSnapshot !== null,
      batchRunning: state.batchSnapshot?.rendererBusy ?? false,
      batchSubmissionPending: state.renderSubmissionTarget === 'batch',
      setBatchOutputFolder: state.setBatchOutputFolder,
      setBatchItemSkipOverlay: state.setBatchItemSkipOverlay,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

/** @returns {object} Inputs that invalidate a reviewed batch: the videos, their offsets, and the activity. */
export function useBatchSyncInputs() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      parsedActivity: state.parsedActivity,
      importedVideos: state.importedVideos,
      videoEdits: state.videoEdits,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
      transitionDefaults: state.transitionDefaults,
      availableCodecs: state.availableCodecs,
    })),
  )
}
