/**
 * Provides the single- and multiple-selection toggle group primitives.
 */

import * as ToggleGroupPrimitive from '@radix-ui/react-toggle-group'
import { cva } from 'class-variance-authority'

import { cn } from '@/lib/utils'

const toggleGroupVariants = cva('inline-flex w-fit items-center rounded-sm border border-input bg-background', {
  variants: {
    size: {
      default: 'p-0.5',
      compact: 'p-[0.1rem] pr-0',
    },
  },
  defaultVariants: {
    size: 'default',
  },
})

const toggleGroupItemVariants = cva(
  'inline-flex cursor-pointer items-center justify-center rounded-xs text-muted-foreground transition-colors hover:bg-surface-elevated hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 data-[state=on]:bg-foreground data-[state=on]:text-surface',
  {
    variants: {
      size: {
        default: 'size-8 [&_svg]:size-4',
        compact: 'size-[1.8rem] [&_svg]:size-[0.9rem]',
      },
    },
    defaultVariants: {
      size: 'default',
    },
  },
)

/**
 * @param {React.ComponentProps<typeof ToggleGroupPrimitive.Root>} props
 * @returns {React.ReactElement}
 */
function ToggleGroup({ className, size, ...props }) {
  return <ToggleGroupPrimitive.Root data-slot="toggle-group" className={cn(toggleGroupVariants({ size }), className)} {...props} />
}

/**
 * @param {React.ComponentProps<typeof ToggleGroupPrimitive.Item>} props
 * @returns {React.ReactElement}
 */
function ToggleGroupItem({ className, size, ...props }) {
  return <ToggleGroupPrimitive.Item data-slot="toggle-group-item" className={cn(toggleGroupItemVariants({ size }), className)} {...props} />
}

export { ToggleGroup, ToggleGroupItem }
