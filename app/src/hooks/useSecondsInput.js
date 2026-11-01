/**
 * Draft, preview and commit of one seconds input (see `components/ui/seconds-input`).
 */

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { bindingStepSeconds, formatSecondsText, normalizeSecondsDraft, parseSecondsDraft, snapSeconds } from '@/lib/seconds-input'
import { formatSecondsClock } from '@/lib/time-format'

/**
 * Keeps the typed draft until it is committed (Enter or leaving the field). The
 * field shows the value as a clock; while a complete draft is typed, the preview
 * reads it as a clock and in seconds. A committed value is snapped to the frame
 * grid right away; the field then shows the stored value again. The
 * `secondsInput.step` shortcut moves the draft and commits once when its key is
 * released, so holding it adds one undo step.
 * @param {object} options Input options.
 * @param {number} options.value Stored value in seconds.
 * @param {(seconds: number) => string|null|undefined} options.onCommit Stores a snapped value; returns a message when it is rejected.
 * @param {{num: number, den: number}|null} options.fps Frame grid the value snaps to, or null.
 * @param {'time'|'duration'} options.mode A time snaps to the nearest frame; a duration is above 0 and at least one frame.
 * @param {boolean} options.signed Whether negative values are allowed.
 * @param {number|null} options.step Seconds the step buttons add or remove, or null without step buttons.
 * @param {(() => void)|null} options.onReset Resets the stored value, or null without a reset button. A reset discards the draft.
 * @returns {{text: string, preview: string|null, error: string|null, onChange: function, onBlur: function, onKeyDown: function,
 *   onKeyUp: function, onStep: function|null, onReset: function|null}} Field model.
 */
export function useSecondsInput({ value, onCommit, fps, mode, signed, step, onReset }) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(null)
  const [error, setError] = useState(null)
  const [steppingKey, setSteppingKey] = useState(null)
  const draftSeconds = draft === null ? null : parseSecondsDraft(draft)

  const rejectionOf = (seconds) => {
    if (mode === 'duration' && seconds <= 0) return t('components.secondsAboveZero', 'Enter a duration above 0 seconds')
    if (!signed && seconds < 0) return t('components.secondsNotNegative', 'Enter a time of 0 seconds or more')
    return null
  }

  const store = (seconds) => {
    const rejection = rejectionOf(seconds)
    if (rejection !== null) {
      setError(rejection)
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
      setError(t('components.secondsInvalid', 'Enter a time like 05:30, a number of seconds, or a sum like 05:30+10'))
      return
    }
    store(draftSeconds)
  }

  const stepDraft = (seconds) => {
    const next = (draftSeconds ?? value) + seconds
    const rejection = rejectionOf(next)
    setError(rejection)
    if (rejection === null) setDraft(formatSecondsClock(snapSeconds(next, fps, mode)))
  }

  return {
    text: draft ?? formatSecondsClock(value),
    preview:
      draftSeconds === null || steppingKey !== null
        ? null
        : t('components.secondsPreview', {
            defaultValue: '= {{clock}} ({{seconds}} s)',
            clock: formatSecondsClock(draftSeconds),
            seconds: formatSecondsText(draftSeconds),
          }),
    error,
    onChange: (event) => {
      const next = normalizeSecondsDraft(event.target.value, signed)
      if (next === null) return
      setDraft(next)
      setError(null)
    },
    onBlur: () => {
      setSteppingKey(null)
      commit()
    },
    onKeyDown: (event) => {
      if (event.key === 'Enter') commit()
      if (event.key === 'Escape') {
        setDraft(null)
        setError(null)
      }
      const match = matchKeyboardShortcut(event, 'seconds-input')
      if (match?.commandId !== 'secondsInput.step') return
      const seconds = bindingStepSeconds(match.binding, fps)
      if (seconds === null) return
      event.preventDefault()
      setSteppingKey(event.key)
      stepDraft(seconds)
    },
    // Matched by key alone: a modifier may be released before the key it stepped with.
    onKeyUp: (event) => {
      if (event.key !== steppingKey) return
      setSteppingKey(null)
      commit()
    },
    onStep: step === null ? null : (direction) => store((draftSeconds ?? value) + direction * step),
    onReset:
      onReset === null
        ? null
        : () => {
            setDraft(null)
            setError(null)
            onReset()
          },
  }
}
