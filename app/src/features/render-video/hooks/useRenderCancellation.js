import { useEffect, useState } from 'react'

/**
 * Tracks an optional progress-panel cancellation request.
 * @param {object} params Render status and optional cancellation callback.
 * @returns {object} Cancellation state and handler.
 */
export default function useRenderCancellation({ status, onCancel }) {
  const [isCancelling, setIsCancelling] = useState(false)

  useEffect(() => {
    if (status !== 'rendering') setIsCancelling(false)
  }, [status])

  const handleCancel = async () => {
    try {
      setIsCancelling(true)
      await onCancel()
    } catch (error) {
      console.error('Failed to cancel render:', error)
      setIsCancelling(false)
    }
  }

  return { isCancelling, handleCancel }
}
