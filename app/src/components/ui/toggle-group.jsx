/**
 * Provides the single- and multiple-selection toggle group primitives.
 */

import * as ToggleGroupPrimitive from '@radix-ui/react-toggle-group'

import { cn } from '@/lib/utils'

/**
 * @param {React.ComponentProps<typeof ToggleGroupPrimitive.Root>} props
 * @returns {React.ReactElement}
 */
function ToggleGroup({ className, ...props }) {
  return (
    <ToggleGroupPrimitive.Root
      data-slot="toggle-group"
      className={cn('inline-flex w-fit items-center rounded-sm border border-input bg-background p-0.5', className)}
      {...props}
    />
  )
}

/**
 * @param {React.ComponentProps<typeof ToggleGroupPrimitive.Item>} props
 * @returns {React.ReactElement}
 */
function ToggleGroupItem({ className, ...props }) {
  return (
    <ToggleGroupPrimitive.Item
      data-slot="toggle-group-item"
      className={cn(
        'inline-flex size-8 cursor-pointer items-center justify-center rounded-xs text-muted-foreground transition-colors hover:bg-surface-elevated hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 data-[state=on]:bg-foreground data-[state=on]:text-surface [&_svg]:size-4',
        className,
      )}
      {...props}
    />
  )
}

export { ToggleGroup, ToggleGroupItem }
