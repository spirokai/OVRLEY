/**
 * Provides reusable blur input UI primitives for the application.
 */

import { useEffect, useRef, useState } from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

/**
 * Renders the blur input component.
 *
 * @param {object} props - Component props.
 * @param {*} props.value - Committed external value. The component keeps a
 * local draft while the user edits and only commits changes on blur or
 * explicit step interactions.
 * @param {*} props.onChange - Callback invoked to change.
 * @param {*} props.onBlur - Callback invoked to blur.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
export function BlurInput({ value: initialValue, onChange, onBlur, className, ...props }) {
  const [value, setValue] = useState(initialValue)
  const inputRef = useRef(null)
  const draftValueRef = useRef(initialValue)
  const isFocusedRef = useRef(false)
  const committedValueRef = useRef(initialValue)
  const isNumberInput = props.type === 'number'

  const setDraftValue = (nextValue) => {
    draftValueRef.current = nextValue
    setValue(nextValue)
  }

  useEffect(() => {
    const draftIsDirty = draftValueRef.current != committedValueRef.current
    committedValueRef.current = initialValue

    // Preserve in-progress drafts when upstream committed state changes.
    if (isFocusedRef.current && draftIsDirty) {
      return
    }

    draftValueRef.current = initialValue
    setValue(initialValue)
  }, [initialValue])

  const handleChange = (e) => {
    setDraftValue(e.target.value)
  }

  const handleBlur = (e) => {
    isFocusedRef.current = false
    // console.log('BlurInput blur:', { value, initialValue, changed: value != initialValue })
    // Relaxed comparison to catch type differences (e.g. "500" vs 500)
    if (value != initialValue) {
      // Mock event object to match expected interface
      onChange?.({ target: { value } })
    }
    onBlur?.(e)
  }

  const handleKeyDown = (e) => {
    if (e.key === 'Enter') {
      inputRef.current?.blur()
    }

    if (isNumberInput && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      requestAnimationFrame(() => {
        if (!inputRef.current) return

        const nextValue = inputRef.current.value
        setDraftValue(nextValue)
        onChange?.({ target: { value: nextValue } })
      })
    }

    props.onKeyDown?.(e)
  }

  const handleStep = (direction) => {
    if (!inputRef.current) {
      return
    }

    if (direction === 'up') {
      inputRef.current.stepUp()
    } else {
      inputRef.current.stepDown()
    }

    const nextValue = inputRef.current.value
    setDraftValue(nextValue)
    onChange?.({ target: { value: nextValue } })
  }

  const inputElement = (
    <Input
      {...props}
      ref={inputRef}
      value={value}
      onFocus={(event) => {
        isFocusedRef.current = true
        props.onFocus?.(event)
      }}
      onChange={handleChange}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      className={cn(
        'transition-colors',
        value !== initialValue && 'border-accent-border bg-surface-accent-soft',
        isNumberInput && 'pr-11 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none',
        className,
      )}
    />
  )

  if (isNumberInput) {
    return (
      <div className="relative">
        {inputElement}
        <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
          <button
            type="button"
            className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => handleStep('up')}
            disabled={props.disabled}
            aria-label="Increase value"
          >
            <ChevronUp className="h-3 w-3" />
          </button>
          <div className="h-px bg-border/60" />
          <button
            type="button"
            className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => handleStep('down')}
            disabled={props.disabled}
            aria-label="Decrease value"
          >
            <ChevronDown className="h-3 w-3" />
          </button>
        </div>
      </div>
    )
  }

  return inputElement
}
