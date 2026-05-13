/**
 * Provides reusable alert UI primitives for the application.
 */

import * as React from 'react'
import { cva } from 'class-variance-authority'

import { cn } from '@/lib/utils'

const alertVariants = cva(
  'relative w-full rounded-lg border px-4 py-3 text-sm grid has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] grid-cols-[0_1fr] has-[>svg]:gap-x-3 gap-y-0.5 items-start [&>svg]:size-4 [&>svg]:translate-y-0.5 [&>svg]:text-current',
  {
    variants: {
      variant: {
        default: 'bg-card text-card-foreground',
        destructive: 'text-destructive bg-card [&>svg]:text-current *:data-[slot=alert-description]:text-destructive/90',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
)

/**
 * Renders the alert component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.variant - Value for variant.
 * @returns {JSX.Element} Rendered component output.
 */
function Alert({ className, variant, ...props }) {
  return <div data-slot="alert" role="alert" className={cn(alertVariants({ variant }), className)} {...props} />
}

/**
 * Renders the alert title component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function AlertTitle({ className, ...props }) {
  return <div data-slot="alert-title" className={cn('col-start-2 line-clamp-1 min-h-4 font-medium tracking-tight', className)} {...props} />
}

/**
 * Renders the alert description component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function AlertDescription({ className, ...props }) {
  return (
    <div
      data-slot="alert-description"
      className={cn('text-muted-foreground col-start-2 grid justify-items-start gap-1 text-sm [&_p]:leading-relaxed', className)}
      {...props}
    />
  )
}

export { Alert, AlertTitle, AlertDescription }
