import { useCallback, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { openSinglePath } from '@/lib/file-dialog'
import { filenameFromSelectedPath } from '@/lib/file-dialog'

const ACTIVITY_FILTER = [{ name: 'Activity', extensions: ['gpx', 'fit', 'tcx', 'srt', 'igc', 'csv', 'vbo', 'uddf', 'ssrf', 'xml'] }]
const VIDEO_FILTER = [{ name: 'Video', extensions: ['mp4', 'mov', 'mkv'] }]

function sourceExtension(path) {
  return String(path).split('.').at(-1)?.toLowerCase() ?? ''
}

function acceptsSourceRole(role, path) {
  const extensions = role === 'video' ? VIDEO_FILTER[0].extensions : ACTIVITY_FILTER[0].extensions
  return extensions.includes(sourceExtension(path))
}

/**
 * Owns the locate/skip/cancel interaction for missing project sources.
 * @returns {object} Missing-source dialog state and project source resolver.
 */
export default function useProjectSourceRecovery() {
  const [missingSource, setMissingSource] = useState(null)
  const pendingRecovery = useRef(null)
  const sourceRole = missingSource?.role ?? null

  const requestRecovery = useCallback(
    (role, path) =>
      new Promise((resolve) => {
        if (pendingRecovery.current) throw new Error('A project source recovery request is already active')
        pendingRecovery.current = resolve
        setMissingSource({ role, fileName: filenameFromSelectedPath(path) })
      }),
    [],
  )

  const completeRecovery = useCallback((result) => {
    const resolve = pendingRecovery.current
    if (!resolve) return
    pendingRecovery.current = null
    setMissingSource(null)
    resolve(result)
  }, [])

  const locateMissingSource = useCallback(async () => {
    if (!sourceRole) return
    const filters = sourceRole === 'video' ? VIDEO_FILTER : ACTIVITY_FILTER
    const path = await openSinglePath(filters)
    if (path) completeRecovery({ action: 'locate', path })
  }, [completeRecovery, sourceRole])

  const resolveSource = useCallback(
    async (role, resolvedPath, sourceExists) => {
      if (!resolvedPath) return null
      if (sourceExists) return resolvedPath

      const recovery = await requestRecovery(role, resolvedPath)
      if (recovery.action === 'cancel') return undefined
      if (recovery.action === 'skip') return null
      if (!acceptsSourceRole(role, recovery.path)) throw new Error(`Selected file is not a supported ${role} source`)
      return recovery.path
    },
    [requestRecovery],
  )

  /**
   * Resolves the activity and every video in order. A skipped video resolves to
   * null; cancelling any source cancels the whole load.
   */
  const resolveProjectSources = useCallback(
    async ({ activityPath, videoPaths }) => {
      const [activityExists, ...videosExist] = await Promise.all([
        activityPath ? backend.selectedPathIsFile(activityPath) : false,
        ...videoPaths.map((videoPath) => backend.selectedPathIsFile(videoPath)),
      ])
      const resolvedActivityPath = await resolveSource('activity', activityPath, activityExists)
      if (resolvedActivityPath === undefined) return null
      const resolvedVideoPaths = []
      for (const [index, videoPath] of videoPaths.entries()) {
        const resolvedVideoPath = await resolveSource('video', videoPath, videosExist[index])
        if (resolvedVideoPath === undefined) return null
        resolvedVideoPaths.push(resolvedVideoPath)
      }
      return { activityPath: resolvedActivityPath, videoPaths: resolvedVideoPaths }
    },
    [resolveSource],
  )

  return {
    dialog: {
      open: Boolean(sourceRole),
      sourceRole: sourceRole ?? undefined,
      fileName: missingSource?.fileName,
      onLocate: locateMissingSource,
      onLoadAnyway: () => completeRecovery({ action: 'skip' }),
      onCancel: () => completeRecovery({ action: 'cancel' }),
    },
    resolveProjectSources,
  }
}
