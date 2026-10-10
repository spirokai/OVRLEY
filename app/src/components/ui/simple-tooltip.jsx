/**
 * Provides reusable simple tooltip UI primitives for the application.
 */

import React, { useState } from 'react'
import { cn } from '@/lib/utils'

/**
 * Renders the simple tooltip component.
 *
 * @param {object} props - Component props.
 * @param {*} props.content - Value for content.
 * @param {*} props.children - Nested React children.
 * @param {*} props.side - Value for side.
 * @param {'start'|'center'|'end'} props.align - Alignment along the tooltip side.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {string} props.contentClassName - Additional class names for the tooltip bubble.
 * @param {number} [props.delayMs=500] - Delay before the fade-in starts, in milliseconds.
 * @returns {JSX.Element} Rendered component output.
 */
export function SimpleTooltip({ content, children, side = 'top', align = 'center', className = '', contentClassName = '', delayMs = 400 }) {
  const [show, setShow] = useState(false)

  if (!content) return children

  const sideClasses = {
    top: 'bottom-full mb-2',
    bottom: 'top-full mt-2',
    left: 'right-full mr-2',
    right: 'left-full ml-2',
  }[side]

  const horizontal = side === 'top' || side === 'bottom'
  const alignment = {
    start: {
      content: horizontal ? 'left-0' : 'top-0',
      arrow: horizontal ? 'left-2 -translate-x-1/2' : 'top-2 -translate-y-1/2',
    },
    center: {
      content: horizontal ? 'left-1/2 -translate-x-1/2' : 'top-1/2 -translate-y-1/2',
      arrow: horizontal ? 'left-1/2 -translate-x-1/2' : 'top-1/2 -translate-y-1/2',
    },
    end: {
      content: horizontal ? 'right-0' : 'bottom-0',
      arrow: horizontal ? 'right-2 translate-x-1/2' : 'bottom-2 translate-y-1/2',
    },
  }[align]

  const arrowClasses = {
    top: 'top-full -mt-px border-t-surface-tooltip',
    bottom: 'bottom-full -mt-px border-b-surface-tooltip',
    left: 'left-full -ml-px border-l-surface-tooltip',
    right: 'right-full -mr-px border-r-surface-tooltip',
  }[side]

  return (
    <div className={`relative inline-flex ${className}`} onMouseEnter={() => setShow(true)} onMouseLeave={() => setShow(false)}>
      {children}
      {show && (
        <div
          className={cn(
            `absolute ${sideClasses} ${alignment.content} z-1000 whitespace-nowrap rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-xs text-foreground shadow-2xl pointer-events-none animate-in fade-in duration-100 fill-mode-both ease-out`,
            contentClassName,
          )}
          style={{ animationDelay: `${delayMs}ms` }}
        >
          {content}
          {/* Arrow */}
          <div className={`absolute border-4 border-transparent ${arrowClasses} ${alignment.arrow}`} />
        </div>
      )}
    </div>
  )
}
