import { useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { readDownloadEvent, readUpdateMetadata } from '../utils/updateMetadata'
import { formatUpdateProgress, getUpdateProgressPercent } from '../utils/updateProgress'

let startupUpdateCheckPromise
let startupUpdateDismissed = false

/**
 * Owns the one startup update check and the user-initiated updater flow.
 *
 * @returns {object} Canonical update prompt state and actions.
 */
export default function useAppUpdate() {
  const [state, setState] = useState({
    phase: 'hidden',
    version: null,
    downloadAndInstall: null,
    progress: null,
    error: null,
  })
  const dismissedRef = useRef(false)

  useEffect(() => {
    if (!backend.hasTauriRuntime()) {
      return undefined
    }

    let active = true
    getStartupUpdate()
      .then((metadata) => {
        if (!active || startupUpdateDismissed || dismissedRef.current || !metadata) {
          return
        }

        setState({
          phase: 'available',
          version: metadata.version,
          downloadAndInstall: metadata.downloadAndInstall,
          progress: null,
          error: null,
        })
      })
      .catch((error) => {
        console.error('Update check failed:', error)
      })

    return () => {
      active = false
    }
  }, [])

  const dismiss = () => {
    dismissedRef.current = true
    startupUpdateDismissed = true
    setState((current) => ({ ...current, phase: 'hidden', downloadAndInstall: null, error: null }))
  }

  const installUpdate = async () => {
    if (state.phase !== 'available' || !state.downloadAndInstall) {
      return
    }

    const downloadAndInstall = state.downloadAndInstall
    let downloadedBytes = 0
    let contentLength = null

    setState((current) => ({
      ...current,
      phase: 'downloading',
      error: null,
      progress: { mode: 'indeterminate', downloadedBytes: 0, contentLength: null },
    }))

    try {
      await downloadAndInstall((event) => {
        const downloadEvent = readDownloadEvent(event)
        switch (downloadEvent.type) {
          case 'started':
            contentLength = downloadEvent.contentLength
            setState((current) => ({
              ...current,
              progress: {
                mode: contentLength > 0 ? 'determinate' : 'indeterminate',
                downloadedBytes,
                contentLength,
              },
            }))
            break
          case 'progress':
            downloadedBytes += downloadEvent.chunkLength
            setState((current) => ({
              ...current,
              progress: {
                mode: contentLength > 0 ? 'determinate' : 'indeterminate',
                downloadedBytes,
                contentLength,
              },
            }))
            break
          case 'finished':
            break
        }
      })

      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (error) {
      console.error('Update installation failed:', error)
      setState((current) => ({
        ...current,
        phase: 'failed',
        error,
      }))
    }
  }

  const progressPercent = state.progress ? getUpdateProgressPercent(state.progress) : 0

  return {
    open: state.phase !== 'hidden',
    phase: state.phase,
    version: state.version,
    progress: state.progress,
    progressPercent,
    progressLabel: state.progress ? formatUpdateProgress(progressPercent) : '',
    error: state.error,
    onUpdateNow: installUpdate,
    onLater: dismiss,
    onClose: dismiss,
  }
}

function getStartupUpdate() {
  if (!startupUpdateCheckPromise) {
    startupUpdateCheckPromise = (async () => {
      const distributionKind = await backend.getDistributionKind()
      if (distributionKind === 'portable') {
        return null
      }

      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check()
      return update ? readUpdateMetadata(update) : null
    })()
  }

  return startupUpdateCheckPromise
}
