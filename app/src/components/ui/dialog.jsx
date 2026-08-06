import { Dialog as DialogPrimitive } from 'radix-ui'
import { cn } from '@/lib/utils'

/**
 * Provides the Radix dialog root.
 *
 * @param {object} props - Dialog root props.
 * @returns {JSX.Element} Rendered dialog root.
 */
function Dialog(props) {
  return <DialogPrimitive.Root data-slot="dialog" {...props} />
}

/**
 * Renders a modal overlay around the dialog content.
 *
 * @param {object} props - Overlay props.
 * @param {*} props.className - Additional overlay classes.
 * @returns {JSX.Element} Rendered dialog overlay.
 */
function DialogOverlay({ className, ...props }) {
  return <DialogPrimitive.Overlay data-slot="dialog-overlay" className={cn(className)} {...props} />
}

/**
 * Renders portaled, accessible dialog content while allowing feature dialogs
 * to retain their established overlay and panel styling.
 *
 * @param {object} props - Dialog content props.
 * @param {*} props.className - Additional content classes.
 * @param {*} props.children - Dialog contents.
 * @param {*} props.overlayClassName - Classes applied to the modal overlay.
 * @returns {JSX.Element} Rendered dialog content.
 */
function DialogContent({ className, children, overlayClassName, ...props }) {
  const portalContainer = typeof document === 'undefined' ? undefined : document.querySelector('.app-shell') || undefined

  return (
    <DialogPrimitive.Portal data-slot="dialog-portal" container={portalContainer}>
      <DialogOverlay className={overlayClassName}>
        <DialogPrimitive.Content data-slot="dialog-content" className={cn('outline-none', className)} {...props}>
          {children}
        </DialogPrimitive.Content>
      </DialogOverlay>
    </DialogPrimitive.Portal>
  )
}

/**
 * Renders the accessible dialog title.
 *
 * @param {object} props - Dialog title props.
 * @param {*} props.className - Additional title classes.
 * @returns {JSX.Element} Rendered dialog title.
 */
function DialogTitle({ className, ...props }) {
  return <DialogPrimitive.Title data-slot="dialog-title" className={cn(className)} {...props} />
}

/**
 * Renders the accessible dialog description.
 *
 * @param {object} props - Dialog description props.
 * @param {*} props.className - Additional description classes.
 * @returns {JSX.Element} Rendered dialog description.
 */
function DialogDescription({ className, ...props }) {
  return <DialogPrimitive.Description data-slot="dialog-description" className={cn(className)} {...props} />
}

export { Dialog, DialogContent, DialogDescription, DialogTitle }
