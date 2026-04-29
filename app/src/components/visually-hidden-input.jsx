/**
 * Renders the visually hidden input portion of the application interface.
 */

/* eslint-disable react-hooks/refs */

'use client'
import * as React from 'react'

/**
 * Renders the visually hidden input component.
 *
 * @param {*} props - Component props.
 * @returns {JSX.Element} Rendered component output.
 */
function VisuallyHiddenInput(props) {
  const {
    control,
    value,
    checked,
    bubbles = true,
    type = 'hidden',
    style,
    ...inputProps
  } = props

  const isCheckInput = React.useMemo(
    () => type === 'checkbox' || type === 'radio' || type === 'switch',
    [type],
  )
  const inputRef = React.useRef(null)

  const prevValueRef = React.useRef({
    value: isCheckInput ? checked : value,
    previous: isCheckInput ? checked : value,
  })

  const prevValue = React.useMemo(() => {
    const currentValue = isCheckInput ? checked : value
    if (prevValueRef.current.value !== currentValue) {
      prevValueRef.current.previous = prevValueRef.current.value
      prevValueRef.current.value = currentValue
    }
    return prevValueRef.current.previous
  }, [isCheckInput, value, checked])

  const [controlSize, setControlSize] = React.useState({})

  React.useLayoutEffect(() => {
    if (!control) {
      setControlSize({})
      return
    }

    setControlSize({
      width: control.offsetWidth,
      height: control.offsetHeight,
    })

    if (typeof window === 'undefined') return

    const resizeObserver = new ResizeObserver((entries) => {
      if (!Array.isArray(entries) || !entries.length) return

      const entry = entries[0]
      if (!entry) return

      let width
      let height

      if ('borderBoxSize' in entry) {
        const borderSizeEntry = entry.borderBoxSize
        const borderSize = Array.isArray(borderSizeEntry)
          ? borderSizeEntry[0]
          : borderSizeEntry
        width = borderSize.inlineSize
        height = borderSize.blockSize
      } else {
        width = control.offsetWidth
        height = control.offsetHeight
      }

      setControlSize({ width, height })
    })

    resizeObserver.observe(control, { box: 'border-box' })
    return () => {
      resizeObserver.disconnect()
    }
  }, [control])

  React.useEffect(() => {
    const input = inputRef.current
    if (!input) return

    const inputProto = window.HTMLInputElement.prototype
    const propertyKey = isCheckInput ? 'checked' : 'value'
    const eventType = isCheckInput ? 'click' : 'input'
    const currentValue = isCheckInput ? checked : value

    const serializedCurrentValue = isCheckInput
      ? checked
      : typeof value === 'object' && value !== null
        ? JSON.stringify(value)
        : value

    const descriptor = Object.getOwnPropertyDescriptor(inputProto, propertyKey)

    const setter = descriptor?.set

    if (prevValue !== currentValue && setter) {
      const event = new Event(eventType, { bubbles })
      setter.call(input, serializedCurrentValue)
      input.dispatchEvent(event)
    }
  }, [prevValue, value, checked, bubbles, isCheckInput])

  const composedStyle = React.useMemo(() => {
    return {
      ...style,
      ...(controlSize.width !== undefined && controlSize.height !== undefined
        ? controlSize
        : {}),
      border: 0,
      clip: 'rect(0 0 0 0)',
      clipPath: 'inset(50%)',
      height: '1px',
      margin: '-1px',
      overflow: 'hidden',
      padding: 0,
      position: 'absolute',
      whiteSpace: 'nowrap',
      width: '1px',
    }
  }, [style, controlSize])

  return (
    <input
      type={type}
      {...inputProps}
      ref={inputRef}
      aria-hidden={isCheckInput}
      tabIndex={-1}
      defaultChecked={isCheckInput ? checked : undefined}
      style={composedStyle}
    />
  )
}

export { VisuallyHiddenInput }
