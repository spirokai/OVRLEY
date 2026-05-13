/**
 * Provides reusable separator UI primitives for the application.
 */

'use client'

import * as React from 'react'
import * as SeparatorPrimitive from '@radix-ui/react-separator'

import { cn } from '@/lib/utils'

/**
 * Renders the separator component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.orientation - Value for orientation.
 * @param {*} props.decorative - Value for decorative.
 * @returns {JSX.Element} Rendered component output.
 */
function Separator({ className, orientation = 'horizontal', decorative = true, ...props }) {
  return (
    <SeparatorPrimitive.Root
      data-slot="separator"
      decorative={decorative}
      orientation={orientation}
      className={cn(
        'bg-border shrink-0 data-[orientation=horizontal]:h-px data-[orientation=horizontal]:w-full data-[orientation=vertical]:h-full data-[orientation=vertical]:w-px',
        className,
      )}
      {...props}
    />
  )
}

export { Separator }
