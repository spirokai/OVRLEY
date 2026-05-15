/**
 * Polls backend for render progress at 500ms intervals while a render is active.
 * Composed by useRenderWorkflow.
 *
 * @param {object} params
 * @param {boolean} params.renderingVideo - Whether a render is currently active.
 * @param {function} params.setRenderProgress - Store setter to update render progress state.
 * @returns {void}
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'

export default function useRenderProgressPolling({ renderingVideo, setRenderProgress }) {
  // Polling — polls backend for render progress via setInterval every 500ms while rendering is active
  useEffect(() => {
    if (!renderingVideo) return

    const pollProgress = async () => {
      try {
        const data = await backend.getRenderProgress()
        const expectedRenderId = useStore.getState().activeRenderId
        if (expectedRenderId === null || expectedRenderId === undefined || data.render_id !== expectedRenderId) {
          return
        }

        setRenderProgress({
          renderId: data.render_id ?? null,
          current: data.current || 0,
          total: data.total || 0,
          encoded: data.encoded || 0,
          status: data.status || 'rendering',
          message: data.message || '',
          estimatedSecondsRemaining: data.estimated_seconds_remaining,
          renderingFps: data.rendering_fps ?? null,
          filename: data.filename || null,
        })
      } catch (error) {
        console.error('Error polling render progress:', error)
      }
    }

    const interval = setInterval(pollProgress, 500)
    pollProgress()
    return () => clearInterval(interval)
  }, [renderingVideo, setRenderProgress])
}
