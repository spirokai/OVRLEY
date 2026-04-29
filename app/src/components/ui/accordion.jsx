/**
 * Provides reusable accordion UI primitives for the application.
 */

import * as React from 'react'
import * as AccordionPrimitive from '@radix-ui/react-accordion'
import { ChevronDown } from 'lucide-react'

import { cn } from '@/lib/utils'

const Accordion = AccordionPrimitive.Root

/**
 * Renders the accordion item component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const AccordionItem = React.forwardRef(({ className, ...props }, ref) => (
  <AccordionPrimitive.Item
    ref={ref}
    className={cn('group/item border-b', className)}
    {...props}
  />
))
AccordionItem.displayName = 'AccordionItem'

/**
 * Renders the accordion trigger component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.children - Nested React children.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const AccordionTrigger = React.forwardRef(
  ({ className, children, ...props }, ref) => (
    <AccordionPrimitive.Header className="flex">
      <AccordionPrimitive.Trigger
        ref={ref}
        className={cn(
          'flex flex-1 items-center justify-between py-4 text-left text-sm font-medium transition-all hover:underline [&[data-state=open]>svg]:rotate-180 [&[data-state=open]>svg]:text-primary',
          className,
        )}
        {...props}
      >
        {children}
        <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200" />
      </AccordionPrimitive.Trigger>
    </AccordionPrimitive.Header>
  ),
)
AccordionTrigger.displayName = AccordionPrimitive.Trigger.displayName

/**
 * Renders the accordion content component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {*} props.children - Nested React children.
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @returns {JSX.Element} Rendered component output.
 */
const AccordionContent = React.forwardRef(
  ({ className, children, ...props }, ref) => (
    <AccordionPrimitive.Content
      ref={ref}
      className="overflow-hidden text-sm data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down"
      {...props}
    >
      <div className={cn('pb-4 pt-0', className)}>{children}</div>
    </AccordionPrimitive.Content>
  ),
)
AccordionContent.displayName = AccordionPrimitive.Content.displayName

export { Accordion, AccordionItem, AccordionTrigger, AccordionContent }
