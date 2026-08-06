/**
 * Renders the keyboard shortcuts help dialog.
 * Pure presentational — all logic is managed by the parent.
 */

import { Keyboard } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { Kbd } from '@/components/ui/kbd'
import { getKeyboardShortcutGroups } from '../utils/keyboardShortcutGroups'

const shortcutGroups = getKeyboardShortcutGroups()

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
        className="flex max-h-[70vh] w-full max-w-2xl flex-col rounded-sm border border-accent-border/80 bg-card/95 py-6 shadow-2xl shadow-background/50"
        aria-describedby={undefined}
      >
        <div className="flex items-center gap-3 px-6">
          <Keyboard className="h-4 w-4 text-primary" />
          <DialogTitle className="text-sm font-semibold text-foreground">Keyboard Shortcuts</DialogTitle>
        </div>
        <p className="py-6 normal-case font-light text-[0.9rem] px-6">
          You can use the following keyboard shortcuts to improve your workflow within OVRLEY:
        </p>

        <div className="min-h-0 flex-1 overflow-y-auto px-6">
          <div className="space-y-8">
            {shortcutGroups.map((category) => (
              <div key={category.name} className="space-y-3">
                <h3 className="text-[1rem] font-extrabold uppercase text-primary">{category.name}</h3>
                <div className="space-y-2">
                  {category.shortcuts.map((shortcut) => (
                    <div key={`${category.name}-${shortcut.description}`} className="grid grid-cols-[12rem_1fr] items-center gap-8">
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        {shortcut.options.map((option) => (
                          <span
                            key={`${shortcut.description}-${option.modifiers.join('|')}-${option.keys.join('|')}`}
                            className="inline-flex items-center gap-1"
                          >
                            {option.modifiers.map((modifier) => (
                              <Kbd key={modifier}>{modifier}</Kbd>
                            ))}
                            {option.modifiers.length && option.keys.length ? <span className="text-[0.9rem] text-muted-foreground">+</span> : null}
                            {option.keys.map((key, keyIndex) => (
                              <span key={`${key}-${keyIndex}`} className="contents">
                                <Kbd>{key}</Kbd>
                              </span>
                            ))}
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

        <div className="mt-6 flex justify-end gap-3 px-6">
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
