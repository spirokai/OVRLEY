/**
 * Provides reusable tabs UI primitives for the application.
 */

import * as React from 'react'
import * as TabsPrimitive from '@radix-ui/react-tabs'
import { cva } from 'class-variance-authority'

import { cn } from '@/lib/utils'

const Tabs = TabsPrimitive.Root

const tabsListVariants = cva('inline-flex items-center justify-center', {
  variants: {
    variant: {
      default: 'h-9 rounded-sm bg-muted p-1 text-muted-foreground',
      main: 'text-muted-foreground border-b border-border',
      toolbar: 'gap-0.5 rounded-xs border border-border/50 p-0.5',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
})

const tabsTriggerVariants = cva(
  'inline-flex cursor-pointer items-center justify-center whitespace-nowrap border border-transparent font-semibold ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default:
          'rounded-sm px-3 py-1 text-sm data-[state=active]:border-primary/0 data-[state=active]:bg-surface-accent-strong data-[state=active]:text-primary',
        main: 'relative px-3 py-4 text-sm after:pointer-events-none after:absolute after:bottom-[-0.148rem] after:left-0 after:right-0 after:z-10 after:h-[0.296rem] after:bg-transparent after:content-[""] data-[state=active]:after:bg-primary data-[state=active]:text-primary',
        toolbar:
          'h-5 rounded-none px-2 text-[0.7rem] uppercase text-muted-foreground hover:bg-surface-elevated hover:text-foreground data-[state=active]:bg-foreground data-[state=active]:text-surface',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
)

/**
 * Renders the tabs list component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.variant - Visual style variant.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const TabsList = React.forwardRef(({ className, variant = 'default', ...props }, ref) => (
  <TabsPrimitive.List ref={ref} className={cn(tabsListVariants({ variant }), className)} {...props} />
))
TabsList.displayName = TabsPrimitive.List.displayName

/**
 * Renders the tabs trigger component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.variant - Visual style variant.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const TabsTrigger = React.forwardRef(({ className, variant = 'default', ...props }, ref) => (
  <TabsPrimitive.Trigger ref={ref} className={cn(tabsTriggerVariants({ variant }), className)} {...props} />
))
TabsTrigger.displayName = TabsPrimitive.Trigger.displayName

/**
 * Renders the tabs content component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const TabsContent = React.forwardRef(({ className, ...props }, ref) => (
  <TabsPrimitive.Content
    ref={ref}
    className={cn(
      'mt-2 ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
      className,
    )}
    {...props}
  />
))
TabsContent.displayName = TabsPrimitive.Content.displayName

export { Tabs, TabsList, TabsTrigger, TabsContent }
