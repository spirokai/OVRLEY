import { useCallback, useRef, useState } from 'react'

/**
 * Owns one pending unsaved-changes confirmation.
 * A command awaits `requestConfirm()` until the mounted dialog answers
 * 'save', 'discard', or 'cancel' through `answerConfirm`.
 * @returns {object} Confirmation request and dialog answer handlers.
 */
export default function useUnsavedChangesConfirm() {
  const resolveRef = useRef(null)
  const [isOpen, setIsOpen] = useState(false)

  const requestConfirm = useCallback(() => {
    setIsOpen(true)
    return new Promise((resolve) => {
      resolveRef.current = resolve
    })
  }, [])

  const answerConfirm = useCallback((action) => {
    resolveRef.current?.(action)
    resolveRef.current = null
    setIsOpen(false)
  }, [])

  return { answerConfirm, isOpen, requestConfirm }
}
