import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'
import { isMacOS } from '@/lib/utils'

/**
 * Tracks whether the header must reserve space for the macOS traffic lights.
 * The macOS window uses an overlay title bar, so the native buttons sit on top of the header
 * except in fullscreen, where macOS hides them.
 *
 * @returns {boolean} True when the traffic lights are drawn over the header.
 */
export default function useTrafficLightInset() {
  const overlayTitleBar = isMacOS() && backend.hasTauriRuntime()
  const [isFullscreen, setIsFullscreen] = useState(false)

  useEffect(() => {
    if (!overlayTitleBar) return undefined

    let disposed = false
    let unlisten

    const registerFullscreenListener = async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const appWindow = getCurrentWindow()
      const syncFullscreen = async () => {
        const fullscreen = await appWindow.isFullscreen()
        if (!disposed) setIsFullscreen(fullscreen)
      }

      // Entering or leaving fullscreen always resizes the window.
      const stopListening = await appWindow.onResized(syncFullscreen)
      if (disposed) {
        stopListening()
        return
      }
      unlisten = stopListening
      await syncFullscreen()
    }

    registerFullscreenListener().catch((error) => console.error('Failed to register fullscreen listener:', error))

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [overlayTitleBar])

  return overlayTitleBar && !isFullscreen
}
