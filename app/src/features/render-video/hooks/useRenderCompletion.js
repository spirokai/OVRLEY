/**
 * Subscribes to render progress store updates and handles
 * render completion, cancellation, and error events.
 * Composed by useRenderWorkflow.
 *
 * @param {object} params
 * @param {boolean} params.renderingVideo - Whether a render is currently active.
 * @param {function} params.setActiveRenderId - Store setter for active render ID.
 * @param {function} params.setRenderingVideo - Store setter for rendering state.
 * @param {function} params.setErrorMessage - Store setter for error messages.
 * @param {function} params.setVideoFilename - Store setter for output filename.
 * @returns {void}
 */

import { useEffect } from 'react'
import useStore from '@/store/useStore'
import * as backend from '@/api/backend'

export default function useRenderCompletion({ renderingVideo, setActiveRenderId, setRenderingVideo, setErrorMessage, setVideoFilename }) {
  // Store subscription for render completion
  useEffect(() => {
    if (!renderingVideo) return

    const unsubscribe = useStore.subscribe(
      (state) => state.renderProgress,
      (nextProgress) => {
        const { activeRenderId: nextActiveRenderId } = useStore.getState()
        if (nextProgress.renderId !== nextActiveRenderId) {
          return
        }

        const { filename, message, status } = nextProgress

        if (status === 'complete' && filename) {
          setVideoFilename(filename)
          setActiveRenderId(null)
          setRenderingVideo(false)
          backend.openVideo(filename).catch((error) => {
            console.error('Error calling open-video:', error)
          })
          return
        }

        if (status === 'cancelled') {
          setActiveRenderId(null)
          setRenderingVideo(false)
          return
        }

        if (status === 'error') {
          setActiveRenderId(null)
          setRenderingVideo(false)
          if (message) {
            setErrorMessage(message)
          }
        }
      },
    )

    return unsubscribe
  }, [renderingVideo, setErrorMessage, setActiveRenderId, setRenderingVideo, setVideoFilename])
}
