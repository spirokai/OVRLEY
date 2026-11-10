/**
 * Provides reusable switch UI primitives for the application.
 */

import * as React from 'react'
import * as SwitchPrimitive from '@radix-ui/react-switch'
import { Check, X } from 'lucide-react'

import { cn } from '@/lib/utils'

const VARIANT_STYLES = {
  default: {
    root: 'data-[state=checked]:bg-primary data-[state=unchecked]:bg-input',
    thumb: 'bg-background data-[state=checked]:bg-primary-foreground data-[state=unchecked]:bg-foreground',
  },
  status: {
    root: 'data-[state=checked]:bg-green-500/80 data-[state=unchecked]:bg-red-400/80',
    thumb: 'flex items-center justify-center bg-white',
  },
}

/**
 * Renders the switch component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {'default'|'status'} [props.variant='default'] - `status` shows on as green with a tick and off as red with a cross.
 * @returns {JSX.Element} Rendered component output.
 */
function Switch({ className, variant = 'default', ...props }) {
  const styles = VARIANT_STYLES[variant]
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        'group peer focus-visible:border-ring focus-visible:ring-ring/50 inline-flex h-[1.15rem] w-8 shrink-0 items-center rounded-full border border-transparent shadow-xs transition-all outline-none focus-visible:ring-[0.222rem] cursor-pointer disabled:cursor-not-allowed disabled:opacity-50',
        styles.root,
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(
          'pointer-events-none block size-4 rounded-full ring-0 transition-transform data-[state=checked]:translate-x-[calc(100%-0.148rem)] data-[state=unchecked]:translate-x-0',
          styles.thumb,
        )}
      >
        {variant === 'status' ? (
          <>
            <Check className="hidden size-3 stroke-3 text-green-600 group-data-[state=checked]:block" aria-hidden="true" />
            <X className="hidden size-3 stroke-3 text-red-500 group-data-[state=unchecked]:block" aria-hidden="true" />
          </>
        ) : null}
      </SwitchPrimitive.Thumb>
    </SwitchPrimitive.Root>
  )
}

export { Switch }
