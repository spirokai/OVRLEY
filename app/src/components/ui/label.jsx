/**
 * Provides reusable label UI primitives for the application.
 */

import * as React from 'react'
import * as LabelPrimitive from '@radix-ui/react-label'

import { cn } from '@/lib/utils'

/**
 * Renders the label component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function Label({ className, disabled = false, ...props }) {
  return (
    <LabelPrimitive.Root
      data-slot="label"
      data-disabled={disabled}
      className={cn(
        'pl-2 flex items-center gap-2 text-sm leading-none font-medium select-none group-data-[disabled=true]:pointer-events-none group-data-[disabled=true]:opacity-50 peer-disabled:cursor-not-allowed peer-disabled:opacity-50 cursor-pointer',
        disabled && 'pointer-events-none cursor-not-allowed opacity-50',
        className,
      )}
      {...props}
    />
  )
}

export { Label }
