/**
 * Renders the app settings dialog with its section menu.
 * Pure presentational — all logic is managed by the parent.
 */

import { X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogClose, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import { SETTINGS_SECTION, SETTINGS_SECTIONS } from '../data/settingsSections'
import GeneralSettingsPanel from './GeneralSettingsPanel'
import KeyboardShortcutsPanel from './KeyboardShortcutsPanel'
import { useTranslation } from 'react-i18next'

/**
 * Renders the settings dialog component.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.open - Whether the dialog is open.
 * @param {string} props.section - Active settings section ID.
 * @param {(section: string) => void} props.onSectionChange - Callback invoked when a menu section is selected.
 * @param {string} props.locale - Active locale code.
 * @param {(locale: string) => void} props.onLocaleChange - Callback invoked when the selected locale changes.
 * @param {Function} props.onClose - Callback invoked to close the dialog.
 * @returns {JSX.Element} Rendered component output.
 */
export default function SettingsDialog({ open, section, onSectionChange, locale, onLocaleChange, onClose }) {
  const { t } = useTranslation()
  const activeSection = SETTINGS_SECTIONS.find((item) => item.id === section)
  const ActiveIcon = activeSection.icon

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
        className="relative flex h-[70vh] max-h-170 w-full max-w-4xl overflow-hidden rounded-sm border border-accent-border/80 bg-card shadow-2xl shadow-background/50"
        aria-describedby={undefined}
      >
        <nav className="flex w-48 shrink-0 flex-col gap-1 border-r border-border/60 p-3" aria-label={t('app-shell.settings', 'Settings')}>
          {SETTINGS_SECTIONS.map((item) => {
            const Icon = item.icon
            const selected = item.id === section
            return (
              <Button
                key={item.id}
                type="button"
                variant={selected ? 'default' : 'ghost'}
                className={cn('h-9 w-full justify-start gap-3 px-3', !selected && 'text-muted-foreground hover:text-foreground')}
                aria-current={selected ? 'page' : undefined}
                onClick={() => onSectionChange(item.id)}
              >
                <Icon className="size-4" />
                {t(item.labelKey)}
              </Button>
            )
          })}
        </nav>

        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="absolute top-4 right-4 h-8 w-8 text-muted-foreground hover:text-foreground"
            aria-label={t('app-shell.close', 'Close')}
          >
            <X className="size-4" />
          </Button>
        </DialogClose>

        <div className="flex min-w-0 flex-1 flex-col pt-6">
          <div className="flex items-center gap-3 pr-16 pl-6">
            <ActiveIcon className="h-4 w-4 shrink-0 text-primary" />
            <DialogTitle className="truncate text-sm font-semibold text-foreground">{t(activeSection.labelKey)}</DialogTitle>
          </div>

          <div className="mt-2 flex min-h-0 flex-1 flex-col">
            {section === SETTINGS_SECTION ? <GeneralSettingsPanel locale={locale} onLocaleChange={onLocaleChange} /> : <KeyboardShortcutsPanel />}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
