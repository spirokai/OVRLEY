import { useEffect, useRef } from 'react'
import * as backend from '@/api/backend'

/**
 * Routes all desktop window-close requests through an asynchronous guard.
 * @param {function} onCloseRequest Resolves whether the window may close.
 * @returns {void}
 */
export default function useWindowCloseGuard(onCloseRequest) {
  const closeRequestRef = useRef(onCloseRequest)
  const handlingClose = useRef(false)
  closeRequestRef.current = onCloseRequest

  useEffect(() => {
    if (!backend.hasTauriRuntime()) return undefined

    let disposed = false
    let unlisten

    const registerCloseGuard = async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const appWindow = getCurrentWindow()
      const stopListening = await appWindow.onCloseRequested(async (event) => {
        event.preventDefault()
        if (handlingClose.current) return

        handlingClose.current = true
        try {
          if (await closeRequestRef.current()) {
            await appWindow.destroy()
          }
        } catch (error) {
          console.error('Failed to close application:', error)
        } finally {
          handlingClose.current = false
        }
      })

      if (disposed) {
        stopListening()
      } else {
        unlisten = stopListening
      }
    }

    registerCloseGuard().catch((error) => console.error('Failed to register window close guard:', error))

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])
}
