import { useState } from 'react'

/**
 * Keeps numeric edits local and validates native bounds/step before committing.
 *
 * @param {{ value: number, onCommit: Function }} options - Displayed value and numeric commit callback.
 * @returns {object} Presentational input props.
 */
export function useInlineNumberInput({ value, onCommit }) {
  const [draft, setDraft] = useState(null)
  const [invalid, setInvalid] = useState(false)

  return {
    value: draft ?? value,
    'aria-invalid': invalid,
    onFocus: (event) => event.currentTarget.select(),
    onChange: (event) => {
      setDraft(event.currentTarget.value)
      setInvalid(false)
    },
    onBlur: (event) => {
      const input = event.currentTarget
      const nextValue = input.valueAsNumber
      // Untouched values can be between slider steps after canvas resizing.
      if (nextValue !== value && !input.checkValidity()) {
        setInvalid(true)
        input.reportValidity()
        return
      }

      setDraft(null)
      setInvalid(false)
      if (nextValue !== value) onCommit(nextValue)
    },
    onKeyDown: (event) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        event.currentTarget.value = String(value)
        event.currentTarget.blur()
      } else if (event.key === 'Enter') {
        event.preventDefault()
        event.currentTarget.blur()
      }
    },
  }
}
