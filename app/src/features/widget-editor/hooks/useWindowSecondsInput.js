import { useState } from 'react'
import { formatWindowSeconds, parseWindowSeconds } from '../utils/rollingChart'

/**
 * Draft text input for a chart window typed as seconds ("45") or minutes and seconds ("1:30").
 * Malformed or out-of-range text stays as an invalid draft instead of committing.
 *
 * @param {{value: number, onCommit: Function}} options - Committed seconds and its commit callback.
 * @returns {object} Props for a text input.
 */
export function useWindowSecondsInput({ value, onCommit }) {
  const [draft, setDraft] = useState(null)
  const invalid = draft !== null && parseWindowSeconds(draft) === null

  return {
    value: draft ?? formatWindowSeconds(value),
    'aria-invalid': invalid,
    onFocus: (event) => event.currentTarget.select(),
    onChange: (event) => setDraft(event.currentTarget.value),
    onBlur: () => {
      if (draft === null || invalid) return
      setDraft(null)
      const seconds = parseWindowSeconds(draft)
      if (seconds !== value) onCommit(seconds)
    },
    onKeyDown: (event) => {
      if (event.key === 'Escape') {
        // Discards the draft; blurring here would commit it from this render's closure.
        event.preventDefault()
        setDraft(null)
      } else if (event.key === 'Enter') {
        event.preventDefault()
        event.currentTarget.blur()
      }
    },
  }
}
