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
      activityFilename: state.activityFilename,
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
      exportCodec: state.exportCodec,
      exportRange: state.exportRange,
      globalDefaults: state.globalDefaults,
      hydrateTemplateState: state.hydrateTemplateState,
      lastSavedTemplateState: state.lastSavedTemplateState,
      loadedTemplateFilename: state.loadedTemplateFilename,
      loadedTemplateSource: state.loadedTemplateSource,
      setErrorMessage: state.setErrorMessage,
      setProcessing: state.setProcessing,
      setLastSavedTemplateState: state.setLastSavedTemplateState,
      setLoadedTemplate: state.setLoadedTemplate,
      templates: state.templates,
      updateRate: state.updateRate,
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
      exportCodec: state.exportCodec,
      exportRange: state.exportRange,
      renderStatus: state.renderProgress.status,
      renderingVideo: state.renderingVideo,
      clearRenderSession: state.clearRenderSession,
      setErrorMessage: state.setErrorMessage,
      setRenderProgress: state.setRenderProgress,
      startRenderSession: state.startRenderSession,
      updateRate: state.updateRate,
    })),
  )
}
