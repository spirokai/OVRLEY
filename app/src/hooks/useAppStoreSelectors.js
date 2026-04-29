import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'

export function useAppShellStore() {
  return useStore(
    useShallow((state) => ({
      config: state.config,
      generatingImage: state.generatingImage,
      globalDefaults: state.globalDefaults,
      setConfig: state.setConfig,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

export function useBootstrapStore() {
  return useStore(
    useShallow((state) => ({
      fetchTemplates: state.fetchTemplates,
      setPlatformOs: state.setPlatformOs,
    })),
  )
}

export function useActivityStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      gpxFilename: state.gpxFilename,
      setErrorMessage: state.setErrorMessage,
      setGeneratingImage: state.setGeneratingImage,
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
      setGeneratingImage: state.setGeneratingImage,
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
      config: state.config,
      exportCodec: state.exportCodec,
      exportRange: state.exportRange,
      renderStatus: state.renderProgress.status,
      renderingVideo: state.renderingVideo,
      setActiveRenderId: state.setActiveRenderId,
      setConfig: state.setConfig,
      setErrorMessage: state.setErrorMessage,
      setExportCodec: state.setExportCodec,
      setExportRange: state.setExportRange,
      setRenderProgress: state.setRenderProgress,
      setRenderingVideo: state.setRenderingVideo,
      setUpdateRate: state.setUpdateRate,
      setVideoFilename: state.setVideoFilename,
      updateRate: state.updateRate,
    })),
  )
}
