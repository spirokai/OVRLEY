import { useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'

function ensureBackendDebugState() {
  if (typeof window === 'undefined') {
    return null
  }

  if (!window.__BACKEND_DEBUG__) {
    window.__BACKEND_DEBUG__ = {
      status: 'initializing',
      error: null,
      logs: [],
      startTime: null,
    }
  }

  return window.__BACKEND_DEBUG__
}

function logBackend(message) {
  const debugState = ensureBackendDebugState()
  const timestamp = new Date().toISOString()

  console.log(`[Backend] ${message}`)

  if (!debugState) {
    return
  }

  debugState.logs.push(`[${timestamp}] ${message}`)
  if (debugState.logs.length > 50) {
    debugState.logs.shift()
  }
}

function updateBackendStatus(status, error = null) {
  const debugState = ensureBackendDebugState()
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
    updateBackendStatus(statusRef.current)

    if (!isTauriRuntime) {
      setBackendReady(true)
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
            logBackend(
              'Backend not yet ready, waiting for Rust commands to respond...',
            )
          }

          const threshold = statusRef.current === 'connecting' ? 90 : 8
          if (
            strikesRef.current >= threshold &&
            statusRef.current !== 'error'
          ) {
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
