/**
 * Renders the new template confirm dialog portion of the application interface.
 */

import { FilePlus2 } from 'lucide-react'
import { Button } from '@/components/ui/button'

/**
 * Renders the new template confirm dialog component.
 *
 * @param {object} props - Component props.
 * @param {*} props.open - Value for open.
 * @param {*} props.onCancel - Callback invoked to cancel.
 * @param {*} props.onConfirm - Callback invoked to confirm.
 * @returns {JSX.Element} Rendered component output.
 */
export default function NewTemplateConfirmDialog({
  open,
  onCancel,
  onConfirm,
}) {
  if (!open) {
    return null
  }

  return (
    <div
      className="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
      onClick={onCancel}
    >
      <div
        className="w-full max-w-md rounded-2xl border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start gap-4">
          <div className="space-y-2">
            <div className="flex items-center gap-4 ">
              <FilePlus2 className="h-4 w-4 text-primary" />
              <h2 className="text-sm font-semibold text-foreground">
                Create New Template
              </h2>
            </div>
            <p className="text-xs leading-5 text-muted-foreground py-2">
              Any unsaved changes will be discarded.
            </p>
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
          <Button
            type="button"
            className="bg-primary text-primary-foreground hover:bg-primary/90"
            onClick={onConfirm}
          >
            New Template
          </Button>
        </div>
      </div>
    </div>
  )
}
