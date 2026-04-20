import { useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'

function ensureSidecarDebugState() {
  if (typeof window === 'undefined') {
    return null
  }

  if (!window.__SIDECAR_DEBUG__) {
    window.__SIDECAR_DEBUG__ = {
      status: 'initializing',
      error: null,
      pid: null,
      logs: [],
      startTime: null,
    }
  }

  return window.__SIDECAR_DEBUG__
}

function logSidecar(message) {
  const debugState = ensureSidecarDebugState()
  const timestamp = new Date().toISOString()

  console.log(`[Sidecar] ${message}`)

  if (!debugState) {
    return
  }

  debugState.logs.push(`[${timestamp}] ${message}`)
  if (debugState.logs.length > 50) {
    debugState.logs.shift()
  }
}

function updateSidecarStatus(status, error = null) {
  const debugState = ensureSidecarDebugState()
  if (!debugState) {
    return
  }

  debugState.status = status
  debugState.error = error
}

export function hasTauriRuntime() {
  return (
    typeof window !== 'undefined' &&
    typeof window.__TAURI_INTERNALS__ !== 'undefined'
  )
}

export default function useBackendStatus() {
  const isTauriRuntime = hasTauriRuntime()
  const [backendStatus, setBackendStatus] = useState(
    isTauriRuntime ? 'connecting' : 'connected',
  )
  const [backendReady, setBackendReady] = useState(!isTauriRuntime)
  const statusRef = useRef(isTauriRuntime ? 'connecting' : 'connected')
  const strikesRef = useRef(0)

  useEffect(() => {
    updateSidecarStatus(statusRef.current)

    if (!isTauriRuntime) {
      setBackendReady(true)
      return undefined
    }

    let isDisposed = false

    const updateStatus = (nextStatus, error = null) => {
      statusRef.current = nextStatus
      setBackendStatus(nextStatus)
      updateSidecarStatus(nextStatus, error)
    }

    const pollBackend = async () => {
      try {
        const socketExists = await backend.socketReady()
        if (!socketExists) {
          strikesRef.current += 1

          if (strikesRef.current === 1) {
            logSidecar('Backend not yet ready, waiting for sidecar to start...')
          }

          const threshold = statusRef.current === 'connecting' ? 90 : 8
          if (
            strikesRef.current >= threshold &&
            statusRef.current !== 'error'
          ) {
            updateStatus('error', 'Backend socket not available')
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
