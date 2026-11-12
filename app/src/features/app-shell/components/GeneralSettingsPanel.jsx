import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { locales } from '@/i18n/locales'
import { useTranslation } from 'react-i18next'

/**
 * Renders the general app settings.
 *
 * @param {object} props - Component props.
 * @param {string} props.locale - Active locale code.
 * @param {(locale: string) => void} props.onLocaleChange - Callback invoked when the selected locale changes.
 * @returns {JSX.Element} Rendered settings panel.
 */
export default function GeneralSettingsPanel({ locale, onLocaleChange }) {
  const { t } = useTranslation()
  const languageLabel = t('app-shell.language', 'Language')

  return (
    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4 thin-scrollbar">
      <div className="flex items-center justify-between gap-6">
        <span className="text-sm text-foreground">{languageLabel}</span>
        <Select value={locale} onValueChange={onLocaleChange}>
          <SelectTrigger className="h-8 w-44 shrink-0 bg-surface text-xs" aria-label={languageLabel}>
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
    </div>
  )
}
