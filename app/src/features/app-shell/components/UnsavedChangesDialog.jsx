/**
 * Renders the unsaved-changes confirmation dialog shared by project and template commands.
 */

import { FilePlus2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

/**
 * Renders the unsaved changes confirm dialog component.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.open - Whether the dialog is open.
 * @param {string} props.title - Dialog title.
 * @param {string} props.description - Dialog body text.
 * @param {string} props.discardLabel - Label of the action that discards changes and proceeds.
 * @param {string} [props.cancelLabel] - Label of the action that aborts.
 * @param {string} [props.saveLabel] - Label of the action that saves changes and proceeds.
 * @param {function} props.onCancel - Callback invoked to cancel.
 * @param {function} props.onSave - Callback invoked to save and proceed.
 * @param {function} props.onDiscard - Callback invoked to discard and proceed.
 * @returns {JSX.Element} Rendered component output.
 */
export default function UnsavedChangesDialog({
  open,
  title,
  description,
  discardLabel,
  cancelLabel = 'Cancel',
  saveLabel = 'Save',
  onCancel,
  onSave,
  onDiscard,
}) {
  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel()
        }
      }}
    >
      <DialogContent
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className="w-full max-w-md rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
      >
        <div className="flex items-start gap-4">
          <div className="space-y-2">
            <div className="flex items-center gap-4 ">
              <FilePlus2 className="h-4 w-4 text-primary" />
              <DialogTitle className="text-sm font-semibold text-foreground">{title}</DialogTitle>
            </div>
            <DialogDescription className="text-xs leading-5 text-muted-foreground py-2">{description}</DialogDescription>
          </div>
        </div>

        <div className="mt-6 flex items-center justify-end gap-3">
          <Button
            type="button"
            variant="outline"
            className="border-border/70 bg-surface text-foreground hover:bg-surface-elevated"
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          <Button type="button" variant="outline" className="border-border/70 bg-surface text-foreground hover:bg-surface-elevated" onClick={onSave}>
            {saveLabel}
          </Button>
          <Button type="button" className="bg-primary text-primary-foreground hover:bg-primary/90" onClick={onDiscard}>
            {discardLabel}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
