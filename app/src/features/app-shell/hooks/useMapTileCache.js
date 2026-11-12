import { useCallback, useEffect, useRef, useState } from 'react'
import { clearMapTileCache, getMapTileCache, hasTauriRuntime, setMapTileCacheLimit } from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * Loads the map tile cache usage when mounted and exposes its limit and clear actions.
 * The cache exists only in the desktop runtime; elsewhere `usage` stays null.
 *
 * @returns {{
 *   usage: import('@/api/backend').MapTileCacheUsage|null,
 *   limitBytes: number|null,
 *   pending: boolean,
 *   setDraftLimitBytes: (limitBytes: number) => void,
 *   commitLimit: (limitBytes: number) => Promise<void>,
 *   clear: () => Promise<void>,
 * }} Cache usage, the slider limit (draft while dragging), and actions.
 */
export default function useMapTileCache() {
  const setErrorMessage = useStore((state) => state.setErrorMessage)
  const [usage, setUsage] = useState(null)
  const [draftLimitBytes, setDraftLimitBytes] = useState(null)
  const [pending, setPending] = useState(false)

  useEffect(() => {
    if (!hasTauriRuntime()) return
    let cancelled = false
    getMapTileCache().then(
      (nextUsage) => {
        if (!cancelled) setUsage(nextUsage)
      },
      (error) => setErrorMessage(error.message),
    )
    return () => {
      cancelled = true
    }
  }, [setErrorMessage])

  // The slider stays interactive during a command, so only the newest command's result is applied.
  const latestCommandRef = useRef(0)
  const runCacheCommand = useCallback(
    async (command) => {
      const commandId = ++latestCommandRef.current
      setPending(true)
      try {
        const nextUsage = await command()
        if (commandId === latestCommandRef.current) setUsage(nextUsage)
      } catch (error) {
        setErrorMessage(error.message)
      } finally {
        if (commandId === latestCommandRef.current) {
          setDraftLimitBytes(null)
          setPending(false)
        }
      }
    },
    [setErrorMessage],
  )

  const commitLimit = useCallback((limitBytes) => runCacheCommand(() => setMapTileCacheLimit(limitBytes)), [runCacheCommand])
  const clear = useCallback(() => runCacheCommand(clearMapTileCache), [runCacheCommand])

  return {
    usage,
    limitBytes: draftLimitBytes ?? usage?.limitBytes ?? null,
    pending,
    setDraftLimitBytes,
    commitLimit,
    clear,
  }
}
