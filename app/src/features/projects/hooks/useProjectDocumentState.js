import { useCallback, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { deepEqual } from '@/store/store-utils'
import useStore from '@/store/useStore'
import { filenameFromSelectedPath } from '@/lib/utils'
import { createProjectContentSnapshot, createProjectDirtyState } from '../utils/projectSnapshot'

function getCurrentProjectContent(state, projectPath) {
  if (!projectPath) return null
  return createProjectContentSnapshot(state, projectPath)
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
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
    })),
  )
  const [loadedProjectPath, setLoadedProjectPath] = useState(null)
  const [lastSavedProjectState, setLastSavedProjectState] = useState(null)
  const savedTimeline = lastSavedProjectState?.timeline ?? null
  const timelineIsModified = useStore(
    useCallback(
      (state) => {
        if (!savedTimeline) return false

        return (
          state.selectedSecond !== savedTimeline.playheadSecond ||
          state.timelineViewport.viewStart !== savedTimeline.viewStart ||
          state.timelineViewport.viewEnd !== savedTimeline.viewEnd
        )
      },
      [savedTimeline],
    ),
  )

  const conflictingOperation = projectOwnedState.isProcessing || projectOwnedState.importingVideo || projectOwnedState.renderingVideo
  const status = useMemo(() => {
    if (!lastSavedProjectState) return 'Unsaved'
    const current = getCurrentProjectContent(projectOwnedState, loadedProjectPath)
    if (!current) return 'Modified'

    const contentIsModified = !deepEqual(current, lastSavedProjectState.content)
    return !contentIsModified && !timelineIsModified ? 'Saved' : 'Modified'
  }, [lastSavedProjectState, loadedProjectPath, projectOwnedState, timelineIsModified])

  const markSaved = useCallback((path, project) => {
    setLoadedProjectPath(path)
    setLastSavedProjectState(createProjectDirtyState(project))
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
    projectName: filenameFromSelectedPath(loadedProjectPath),
    status,
  }
}
