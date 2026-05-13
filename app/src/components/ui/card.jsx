/**
 * Provides reusable card UI primitives for the application.
 */

import * as React from 'react'

import { cn } from '@/lib/utils'

/**
 * Renders the card component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function Card({ className, ...props }) {
  return (
    <div data-slot="card" className={cn('bg-card text-card-foreground flex flex-col gap-6 rounded-xl border py-6 shadow-sm', className)} {...props} />
  )
}

/**
 * Renders the card header component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardHeader({ className, ...props }) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        '@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6',
        className,
      )}
      {...props}
    />
  )
}

/**
 * Renders the card title component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardTitle({ className, ...props }) {
  return <div data-slot="card-title" className={cn('leading-none font-semibold', className)} {...props} />
}

/**
 * Renders the card description component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardDescription({ className, ...props }) {
  return <div data-slot="card-description" className={cn('text-muted-foreground text-sm', className)} {...props} />
}

/**
 * Renders the card action component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardAction({ className, ...props }) {
  return <div data-slot="card-action" className={cn('col-start-2 row-span-2 row-start-1 self-start justify-self-end', className)} {...props} />
}

/**
 * Renders the card content component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardContent({ className, ...props }) {
  return <div data-slot="card-content" className={cn('px-6', className)} {...props} />
}

/**
 * Renders the card footer component.
 *
 * @param {object} props - Component props.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
function CardFooter({ className, ...props }) {
  return <div data-slot="card-footer" className={cn('flex items-center px-6 [.border-t]:pt-6', className)} {...props} />
}

export { Card, CardHeader, CardFooter, CardTitle, CardAction, CardDescription, CardContent }
