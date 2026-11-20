import { useCallback, useEffect, useRef, useState } from 'react'

const COPIED_FEEDBACK_MS = 1500

/**
 * Copies text to the system clipboard and briefly reports success.
 *
 * @returns {{copied: boolean, copy: (text: string) => Promise<void>}} Copy action and transient copied flag.
 */
export default function useCopyToClipboard() {
  const [copied, setCopied] = useState(false)
  const resetTimerRef = useRef(null)

  useEffect(() => () => clearTimeout(resetTimerRef.current), [])

  const copy = useCallback(async (text) => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    clearTimeout(resetTimerRef.current)
    resetTimerRef.current = setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS)
  }, [])

  return { copied, copy }
}
