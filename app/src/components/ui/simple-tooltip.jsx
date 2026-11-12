/**
 * Provides reusable simple tooltip UI primitives for the application.
 */

import React, { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'

/** Space between the trigger and the bubble, and the least space kept to the window edges, in pixels. */
const GAP_PX = 8
const VIEWPORT_MARGIN_PX = 8
/** Least distance of the arrow from a bubble corner, in pixels. */
const ARROW_INSET_PX = 8

const clamp = (value, min, max) => Math.min(Math.max(value, min), Math.max(min, max))

/**
 * Places the bubble beside its trigger in window coordinates, kept inside the window, with the
 * arrow still pointing at the trigger after any shift.
 * @returns {{left: number, top: number, arrow: number}} Bubble position and arrow offset along its edge.
 */
function placeBubble(anchor, bubble, side, align) {
  const horizontal = side === 'top' || side === 'bottom'
  if (horizontal) {
    const top = side === 'top' ? anchor.top - GAP_PX - bubble.height : anchor.bottom + GAP_PX
    const target = align === 'start' ? anchor.left + ARROW_INSET_PX : align === 'end' ? anchor.right - ARROW_INSET_PX : anchor.left + anchor.width / 2
    const preferred = align === 'start' ? anchor.left : align === 'end' ? anchor.right - bubble.width : target - bubble.width / 2
    const left = clamp(preferred, VIEWPORT_MARGIN_PX, window.innerWidth - VIEWPORT_MARGIN_PX - bubble.width)
    return { left, top, arrow: clamp(target - left, ARROW_INSET_PX, bubble.width - ARROW_INSET_PX) }
  }
  const left = side === 'left' ? anchor.left - GAP_PX - bubble.width : anchor.right + GAP_PX
  const target = align === 'start' ? anchor.top + ARROW_INSET_PX : align === 'end' ? anchor.bottom - ARROW_INSET_PX : anchor.top + anchor.height / 2
  const preferred = align === 'start' ? anchor.top : align === 'end' ? anchor.bottom - bubble.height : target - bubble.height / 2
  const top = clamp(preferred, VIEWPORT_MARGIN_PX, window.innerHeight - VIEWPORT_MARGIN_PX - bubble.height)
  return { left, top, arrow: clamp(target - top, ARROW_INSET_PX, bubble.height - ARROW_INSET_PX) }
}

/**
 * Renders the simple tooltip component. The bubble is portaled to the document body and placed in
 * window coordinates, so scrolling or clipping containers neither cut it off nor grow scrollbars.
 *
 * @param {object} props - Component props.
 * @param {*} props.content - Value for content.
 * @param {*} props.children - Nested React children.
 * @param {*} props.side - Value for side.
 * @param {'start'|'center'|'end'} props.align - Alignment along the tooltip side.
 * @param {*} props.className - Class names merged into the wrapper (e.g. `block min-w-0` to keep a truncating child's layout).
 * @param {string} props.contentClassName - Additional class names for the tooltip bubble.
 * @param {number} [props.delayMs=500] - Delay before the fade-in starts, in milliseconds.
 * @param {boolean} [props.wrapText=false] - Wraps long content such as a file path instead of keeping it on one line.
 * @returns {JSX.Element} Rendered component output.
 */
export function SimpleTooltip({
  content,
  children,
  side = 'top',
  align = 'center',
  className = '',
  contentClassName = '',
  delayMs = 400,
  wrapText = false,
}) {
  const [show, setShow] = useState(false)
  const [position, setPosition] = useState(null)
  const wrapperRef = useRef(null)
  const bubbleRef = useRef(null)

  // The bubble renders unseen first, so its size is known before it is placed.
  useLayoutEffect(() => {
    if (!show || bubbleRef.current === null) {
      setPosition(null)
      return
    }
    setPosition(placeBubble(wrapperRef.current.getBoundingClientRect(), bubbleRef.current.getBoundingClientRect(), side, align))
  }, [show, content, side, align])

  // A placed bubble would drift from its trigger, so scrolling or resizing hides it.
  useEffect(() => {
    if (!show) return undefined
    const hide = () => setShow(false)
    window.addEventListener('scroll', hide, true)
    window.addEventListener('resize', hide)
    return () => {
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
    }
  }, [show])

  if (!content) return children

  const horizontal = side === 'top' || side === 'bottom'
  const arrowClasses = {
    top: 'top-full -mt-px border-t-surface-tooltip -translate-x-1/2',
    bottom: 'bottom-full -mb-px border-b-surface-tooltip -translate-x-1/2',
    left: 'left-full -ml-px border-l-surface-tooltip -translate-y-1/2',
    right: 'right-full -mr-px border-r-surface-tooltip -translate-y-1/2',
  }[side]

  return (
    <div ref={wrapperRef} className={cn('relative inline-flex', className)} onMouseEnter={() => setShow(true)} onMouseLeave={() => setShow(false)}>
      {children}
      {show &&
        createPortal(
          <div
            ref={bubbleRef}
            role="tooltip"
            className={cn(
              'fixed z-1000 whitespace-nowrap rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-xs text-foreground shadow-2xl pointer-events-none animate-in fade-in duration-100 fill-mode-both ease-out',
              wrapText && 'w-max max-w-80 whitespace-normal break-all',
              contentClassName,
            )}
            style={{
              left: position?.left ?? 0,
              top: position?.top ?? 0,
              visibility: position === null ? 'hidden' : 'visible',
              animationDelay: `${delayMs}ms`,
            }}
          >
            {content}
            <div
              className={`absolute border-4 border-transparent ${arrowClasses}`}
              style={horizontal ? { left: position?.arrow ?? 0 } : { top: position?.arrow ?? 0 }}
            />
          </div>,
          document.body,
        )}
    </div>
  )
}
