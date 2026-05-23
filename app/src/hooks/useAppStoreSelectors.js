/**
 * Implements the use App Store Selectors hook and related behavior for the app.
 */

import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'

/**
 * Provides layout store state and actions.
 * @returns {*} Result produced by the helper.
 */
export function useLayoutStore() {
  return useStore(
    useShallow((state) => ({
      widgetDrawerOpen: state.widgetDrawerOpen,
      toggleWidgetDrawer: state.toggleWidgetDrawer,
    })),
  )
}

/**
 * Provides app shell store state and actions.
 * @returns {*} Result produced by the helper.
 */
export function useAppShellStore() {
  return useStore(
    useShallow((state) => ({
      config: state.config,
      isProcessing: state.isProcessing,
      globalDefaults: state.globalDefaults,
      importingVideo: state.importingVideo,
      setConfig: state.setConfig,
      setErrorMessage: state.setErrorMessage,
    })),
  )
}

/**
 * Provides bootstrap store state and actions.
 * @returns {*} Result produced by the helper.
 */
export function useBootstrapStore() {
  return useStore(
    useShallow((state) => ({
      fetchAvailableCodecs: state.fetchAvailableCodecs,
      fetchTemplates: state.fetchTemplates,
      setPlatformOs: state.setPlatformOs,
    })),
  )
}

/**
 * Provides activity store state and actions.
 * @returns {*} Result produced by the helper.
 */
export function useActivityStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      gpxFilename: state.gpxFilename,
      setErrorMessage: state.setErrorMessage,
      setProcessing: state.setProcessing,
    })),
  )
}

/**
 * Provides template store state and actions.
 * @returns {*} Result produced by the helper.
 */
export function useTemplateStore() {
  return useStore(
    useShallow((state) => ({
      aspectRatio: state.aspectRatio,
      config: state.config,
      createNewTemplate: state.createNewTemplate,
      exportCodec: state.exportCodec,
      exportRange: state.exportRange,
      fetchTemplates: state.fetchTemplates,
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

/**
 * Provides render store state and actions.
 * @returns {*} Result produced by the helper.
 */
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
