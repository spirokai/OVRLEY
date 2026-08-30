import { useCallback, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { deepEqual } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { filenameFromPath } from '../utils/projectPaths'
import { createProjectDirtyProjection, createProjectSnapshot } from '../utils/projectSnapshot'

function currentDirtyProjection(state, projectPath) {
  if (!projectPath) return null
  return createProjectDirtyProjection(createProjectSnapshot(state, projectPath))
}

/**
 * Owns loaded-project identity, its saved baseline, and derived save status.
 * @returns {object} Project identity, status, and transition callbacks.
 */
export default function useProjectDocumentState() {
  const projectOwnedState = useStore(
    useShallow((state) => ({
      activitySource: state.activitySource,
      config: state.config,
      globalDefaults: state.globalDefaults,
      importedVideoPath: state.importedVideoPath,
      importingVideo: state.importingVideo,
      isProcessing: state.isProcessing,
      renderSettings: state.renderSettings,
      renderingVideo: state.renderingVideo,
      selectedSecond: state.selectedSecond,
      timelineViewport: state.timelineViewport,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
    })),
  )
  const [loadedProjectPath, setLoadedProjectPath] = useState(null)
  const [lastSavedProjectState, setLastSavedProjectState] = useState(null)

  const conflictingOperation = projectOwnedState.isProcessing || projectOwnedState.importingVideo || projectOwnedState.renderingVideo
  const status = useMemo(() => {
    if (!lastSavedProjectState) return 'Unsaved'
    const current = currentDirtyProjection(projectOwnedState, loadedProjectPath)
    return current && deepEqual(current, lastSavedProjectState) ? 'Saved' : 'Modified'
  }, [lastSavedProjectState, loadedProjectPath, projectOwnedState])

  const markSaved = useCallback((path, project) => {
    setLoadedProjectPath(path)
    setLastSavedProjectState(createProjectDirtyProjection(project))
  }, [])

  const markNew = useCallback(() => {
    setLoadedProjectPath(null)
    setLastSavedProjectState(null)
  }, [])

  return {
    conflictingOperation,
    loadedProjectPath,
    markNew,
    markSaved,
    projectName: filenameFromPath(loadedProjectPath),
    status,
  }
}
