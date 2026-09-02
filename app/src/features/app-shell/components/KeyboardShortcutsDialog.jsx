/**
 * Renders the keyboard shortcuts help dialog.
 * Pure presentational — all logic is managed by the parent.
 */

import { useMemo } from 'react'
import { Keyboard } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { Kbd } from '@/components/ui/kbd'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { locales } from '@/i18n/locales'
import { getKeyboardShortcutGroups } from '../utils/keyboardShortcutGroups'
import { useTranslation } from 'react-i18next'

/**
 * Renders the keyboard shortcuts dialog component.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.open - Whether the dialog is open.
 * @param {string} props.locale - Active locale code.
 * @param {(locale: string) => void} props.onLocaleChange - Callback invoked when the selected locale changes.
 * @param {Function} props.onClose - Callback invoked to close the dialog.
 * @returns {JSX.Element} Rendered component output.
 */
export default function KeyboardShortcutsDialog({ open, locale, onLocaleChange, onClose }) {
  const { t } = useTranslation()
  const shortcutGroups = useMemo(() => getKeyboardShortcutGroups(t), [t])
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
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className="flex h-[80vh] max-h-200 w-full max-w-3xl flex-col rounded-sm border border-accent-border/80 bg-card/95 py-6 shadow-2xl shadow-background/50"
        aria-describedby={undefined}
      >
        <div className="flex items-center justify-between gap-3 px-6">
          <div className="flex min-w-0 items-center gap-3">
            <Keyboard className="h-4 w-4 shrink-0 text-primary" />
            <DialogTitle className="truncate text-sm font-semibold text-foreground">
              {t('app-shell.keyboardShortcuts', 'Keyboard Shortcuts')}
            </DialogTitle>
          </div>
          <Select value={locale} onValueChange={onLocaleChange}>
            <SelectTrigger className="h-8 w-44 shrink-0 bg-surface text-xs" aria-label={t('app-shell.language', 'Language')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="end">
              {locales.map(({ value, label }) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <p className="pt-6 pb-3 normal-case font-light text-[0.9rem] px-6">
          {t('app-shell.keyboardShortcutsDescription', 'You can use the following keyboard shortcuts to improve your workflow within OVRLEY:')}
        </p>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 border-t border-b border-border/60">
          <div className="space-y-8 py-4">
            {shortcutGroups.map((category) => (
              <div key={category.name} className="space-y-3">
                <h3 className="text-[1rem] font-extrabold uppercase text-primary">{category.name}</h3>
                <div className="space-y-2">
                  {category.shortcuts.map((shortcut) => (
                    <div key={`${category.name}-${shortcut.description}`} className="grid grid-cols-[16rem_1fr] items-center gap-8">
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        {shortcut.options.map((option) => (
                          <span
                            key={`${shortcut.description}-${option.modifiers.join('|')}-${option.keys.join('|')}`}
                            className="inline-flex items-center gap-1"
                          >
                            {option.modifiers.map((modifier, modifierIndex) => (
                              <span key={modifier} className="inline-flex items-center gap-1">
                                <Kbd>{modifier}</Kbd>
                                {modifierIndex < option.modifiers.length - 1 ? <span className="text-[0.9rem] text-muted-foreground">+</span> : null}
                              </span>
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
            {t('app-shell.close', 'Close')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
