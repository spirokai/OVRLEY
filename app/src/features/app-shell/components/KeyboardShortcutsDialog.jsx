/**
 * Renders the keyboard shortcuts help dialog.
 * Pure presentational — all logic is managed by the parent.
 */

import { Keyboard } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { Kbd } from '@/components/ui/kbd'
import shortcutsData from '../data/keyboardShortcuts.json'

/**
 * Renders the keyboard shortcuts dialog component.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.open - Whether the dialog is open.
 * @param {Function} props.onClose - Callback invoked to close the dialog.
 * @returns {JSX.Element} Rendered component output.
 */
export default function KeyboardShortcutsDialog({ open, onClose }) {
  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose()
        }
      }}
    >
      <DialogContent
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
        className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        aria-describedby={undefined}
      >
        <div className="flex items-center gap-3">
          <Keyboard className="h-4 w-4 text-primary" />
          <DialogTitle className="text-sm font-semibold text-foreground">Keyboard Shortcuts</DialogTitle>
        </div>
        <p className="pt-6 normal-case font-light text-[0.9rem]">
          You can use the following keyboard shortcuts to improve your workflow within OVRLEY:
        </p>

        <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
          <div className="space-y-8">
            {shortcutsData.categories.map((category) => (
              <div key={category.name} className="space-y-3">
                <h3 className="text-[1rem] font-extrabold uppercase text-primary">{category.name}</h3>
                <div className="space-y-2">
                  {category.shortcuts.map((shortcut) => (
                    <div key={`${category.name}-${shortcut.description}`} className="grid grid-cols-[12rem_1fr] items-center gap-8">
                      <div className="flex items-center gap-1">
                        {shortcut.keys.map((key, index) => (
                          <span key={`${key}-${index}`} className="contents">
                            <Kbd>{key}</Kbd>
                            {index < shortcut.keys.length - 1 && <span className="text-[0.9rem] text-muted-foreground">+</span>}
                          </span>
                        ))}
                      </div>
                      <span className="text-xs text-foreground">{shortcut.description}</span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-3">
          <Button
            type="button"
            variant="outline"
            className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
            onClick={onClose}
          >
            Close
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
