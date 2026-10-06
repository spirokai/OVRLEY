import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'

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
      importedVideoPath: state.importedVideoPath,
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
      parsedActivitySource: state.parsedActivitySource,
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

export function useRenderStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      activeRenderId: state.activeRenderId,
      activeRenderOutputPath: state.activeRenderOutputPath,
      config: state.config,
      renderSettings: state.renderSettings,
      renderStatus: state.renderProgress.status,
      renderingVideo: state.renderingVideo,
      clearRenderSession: state.clearRenderSession,
      setErrorMessage: state.setErrorMessage,
      setRenderProgress: state.setRenderProgress,
      startRenderSession: state.startRenderSession,
      setRenderSettings: state.setRenderSettings,
    })),
  )
}

/** @returns {object} Batch choices, native lifecycle and configuration actions. */
export function useBatchRenderStore() {
  return useStore(
    useShallow((state) => ({
      batchVideoFolder: state.batchVideoFolder,
      batchOutputFolder: state.batchOutputFolder,
      batchQueue: state.batchQueue,
      batchSnapshot: state.batchSnapshot,
      batchRunning: state.batchSnapshot?.rendererBusy ?? false,
      batchSubmissionPending: state.batchSubmissionPending,
      setBatchVideoFolder: state.setBatchVideoFolder,
      setBatchOutputFolder: state.setBatchOutputFolder,
      setBatchQueueFromPaths: state.setBatchQueueFromPaths,
      removeBatchQueueItem: state.removeBatchQueueItem,
      clearBatchQueue: state.clearBatchQueue,
      setBatchItemSkipOverlay: state.setBatchItemSkipOverlay,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

/** @returns {object} Inputs that invalidate reviewed batch synchronization. */
export function useBatchSyncInputs() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      parsedActivitySource: state.parsedActivitySource,
      importedVideoPath: state.importedVideoPath,
      importedVideoCreationTime: state.importedVideoCreationTime,
      importedVideoTimeSource: state.importedVideoTimeSource,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
      availableCodecs: state.availableCodecs,
    })),
  )
}
