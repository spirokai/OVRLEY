/**
 * Draft, readout and commit of one seconds input (see `components/ui/seconds-input`).
 */

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { formatSecondsText, normalizeSecondsDraft, parseSecondsDraft, snapSeconds } from '@/lib/seconds-input'
import { formatSecondsClock } from '@/lib/time-format'

/**
 * Keeps the typed draft until it is committed (Enter or leaving the field). A
 * committed value is snapped to the frame grid right away; the field then shows
 * the stored value again.
 * @param {object} options Input options.
 * @param {number} options.value Stored value in seconds.
 * @param {(seconds: number) => string|null|undefined} options.onCommit Stores a snapped value; returns a message when it is rejected.
 * @param {{num: number, den: number}|null} options.fps Frame grid the value snaps to, or null.
 * @param {'time'|'duration'} options.mode A time snaps to the nearest frame; a duration is above 0 and at least one frame.
 * @param {boolean} options.signed Whether negative values are allowed.
 * @param {number|null} options.step Seconds the step buttons add or remove, or null without step buttons.
 * @returns {{text: string, readout: string, error: string|null, onChange: function, onBlur: function, onKeyDown: function, onStep: function|null}}
 *   Field model.
 */
export function useSecondsInput({ value, onCommit, fps, mode, signed, step }) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(null)
  const [error, setError] = useState(null)
  const draftSeconds = draft === null ? null : parseSecondsDraft(draft)

  const store = (seconds) => {
    if (mode === 'duration' && seconds <= 0) {
      setError(t('components.secondsAboveZero', 'Enter a duration above 0 seconds'))
      return
    }
    const snapped = snapSeconds(seconds, fps, mode)
    const failure = snapped === value ? null : (onCommit(snapped) ?? null)
    setError(failure)
    if (failure === null) setDraft(null)
  }

  const commit = () => {
    if (draft === null) return
    if (draftSeconds === null) {
      setError(t('components.secondsInvalid', 'Enter a number of seconds'))
      return
    }
    store(draftSeconds)
  }

  return {
    text: draft ?? formatSecondsText(value),
    readout: formatSecondsClock(draftSeconds ?? value),
    error,
    onChange: (event) => {
      const next = normalizeSecondsDraft(event.target.value, signed)
      if (next === null) return
      setDraft(next)
      setError(null)
    },
    onBlur: commit,
    onKeyDown: (event) => {
      if (event.key === 'Enter') commit()
      if (event.key === 'Escape') {
        setDraft(null)
        setError(null)
      }
    },
    onStep: step === null ? null : (direction) => store((draftSeconds ?? value) + direction * step),
  }
}
