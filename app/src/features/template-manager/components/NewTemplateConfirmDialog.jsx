/**
 * Renders the new template confirm dialog portion of the application interface.
 */

import { FilePlus2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

/**
 * Renders the new template confirm dialog component.
 *
 * @param {object} props - Component props.
 * @param {*} props.open - Value for open.
 * @param {*} props.onCancel - Callback invoked to cancel.
 * @param {*} props.onConfirm - Callback invoked to confirm.
 * @returns {JSX.Element} Rendered component output.
 */
export default function NewTemplateConfirmDialog({ open, onCancel, onConfirm }) {
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
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
        className="w-full max-w-md rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
      >
        <div className="flex items-start gap-4">
          <div className="space-y-2">
            <div className="flex items-center gap-4 ">
              <FilePlus2 className="h-4 w-4 text-primary" />
              <DialogTitle className="text-sm font-semibold text-foreground">Create New Template</DialogTitle>
            </div>
            <DialogDescription className="text-xs leading-5 text-muted-foreground py-2">Any unsaved changes will be discarded.</DialogDescription>
          </div>
        </div>

        <div className="mt-6 flex items-center justify-end gap-3">
          <Button
            type="button"
            variant="outline"
            className="border-border/70 bg-surface text-foreground hover:bg-surface-elevated"
            onClick={onCancel}
          >
            Cancel
          </Button>
          <Button type="button" className="bg-primary text-primary-foreground hover:bg-primary/90" onClick={onConfirm}>
            New Template
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
