/**
 * Provides reusable simple tooltip UI primitives for the application.
 */

import React, { useState } from 'react'

/**
 * Renders the simple tooltip component.
 *
 * @param {object} props - Component props.
 * @param {*} props.content - Value for content.
 * @param {*} props.children - Nested React children.
 * @param {*} props.side - Value for side.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
export function SimpleTooltip({ content, children, side = 'top', className = '' }) {
  const [show, setShow] = useState(false)

  if (!content) return children

  const sideClasses = side === 'top' ? 'bottom-full left-1/2 -translate-x-1/2 mb-2' : 'top-full left-1/2 -translate-x-1/2 mt-2'

  const arrowClasses = side === 'top' ? 'top-full border-t-surface-tooltip' : 'bottom-full border-b-surface-tooltip'

  return (
    <div className={`relative inline-flex ${className}`} onMouseEnter={() => setShow(true)} onMouseLeave={() => setShow(false)}>
      {children}
      {show && (
        <div
          className={`absolute ${sideClasses} z-[1000] whitespace-nowrap rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-xs text-foreground shadow-2xl pointer-events-none animate-in fade-in zoom-in-95 duration-200`}
        >
          {content}
          {/* Arrow */}
          <div className={`absolute left-1/2 -translate-x-1/2 -mt-px border-4 border-transparent ${arrowClasses}`} />
        </div>
      )}
    </div>
  )
}
