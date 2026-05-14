/**
 * Provides backend status state — polls the backend health endpoint and tracks connection state.
 */

import { useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { hasTauriRuntime, logBackend } from '../utils/backendDebug'
import { updateBackendStatus } from '../utils/backendDebug'

/**
 * Provides backend status state and actions.
 * @returns {{ backendReady: boolean, backendStatus: string }} Backend readiness and status.
 */
export default function useBackendStatus() {
  const isTauriRuntime = hasTauriRuntime()
  const [backendStatus, setBackendStatus] = useState(isTauriRuntime ? 'connecting' : 'error')
  const [backendReady, setBackendReady] = useState(false)
  const statusRef = useRef(isTauriRuntime ? 'connecting' : 'error')
  const strikesRef = useRef(0)

  // Polling — checks backend health via IPC every 2 seconds while the component is mounted
  useEffect(() => {
    updateBackendStatus(statusRef.current)

    if (!isTauriRuntime) {
      setBackendStatus('error')
      setBackendReady(false)
      updateBackendStatus('error', 'OVRLEY desktop runtime is required')
      return undefined
    }

    let isDisposed = false

    const updateStatus = (nextStatus, error = null) => {
      statusRef.current = nextStatus
      setBackendStatus(nextStatus)
      updateBackendStatus(nextStatus, error)
    }

    const pollBackend = async () => {
      try {
        const socketExists = await backend.socketReady()
        if (!socketExists) {
          strikesRef.current += 1

          if (strikesRef.current === 1) {
            logBackend('Backend not yet ready, waiting for Rust commands to respond...')
          }

          const threshold = statusRef.current === 'connecting' ? 90 : 8
          if (strikesRef.current >= threshold && statusRef.current !== 'error') {
            updateStatus('error', 'Backend bridge not available')
            setBackendReady(false)
          }
          return
        }

        const health = await backend.healthCheck()
        if (isDisposed) {
          return
        }

        strikesRef.current = 0
        updateStatus('connected')
        setBackendReady(Boolean(health?.ready))
      } catch (error) {
        if (isDisposed) {
          return
        }

        strikesRef.current += 1
        const threshold = statusRef.current === 'connecting' ? 90 : 8
        if (strikesRef.current >= threshold && statusRef.current !== 'error') {
          updateStatus('error', error?.message || 'Backend unavailable')
          setBackendReady(false)
        }
      }
    }

    pollBackend()
    const intervalId = window.setInterval(pollBackend, 2000)

    return () => {
      isDisposed = true
      window.clearInterval(intervalId)
    }
  }, [isTauriRuntime])

  return {
    backendReady,
    backendStatus,
  }
}
