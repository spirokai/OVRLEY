import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { locales } from '@/i18n/locales'
import { useUnitSystem } from '@/hooks/useAppStoreSelectors'
import { changeUnitSystemPreference } from '@/lib/unit-system-preference'
import { UNIT_SYSTEMS } from '@/lib/widget/unit-system'
import { useTranslation } from 'react-i18next'
import useMapTileCache from '../hooks/useMapTileCache'
import MapTileCacheSettings from './MapTileCacheSettings'

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
  const unitSystemLabel = t('app-shell.unitSystem', 'Units')
  const unitSystem = useUnitSystem()
  const mapTileCache = useMapTileCache()

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
      <div className="flex items-center justify-between gap-6">
        <span className="text-sm text-foreground">{unitSystemLabel}</span>
        <Select value={unitSystem} onValueChange={changeUnitSystemPreference}>
          <SelectTrigger className="h-8 w-44 shrink-0 bg-surface text-xs" aria-label={unitSystemLabel}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="end">
            {UNIT_SYSTEMS.map((system) => (
              <SelectItem key={system} value={system}>
                {t(`app-shell.unitSystems.${system}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {mapTileCache.usage ? (
        <MapTileCacheSettings
          usage={mapTileCache.usage}
          limitBytes={mapTileCache.limitBytes}
          pending={mapTileCache.pending}
          onLimitDraft={mapTileCache.setDraftLimitBytes}
          onLimitCommit={mapTileCache.commitLimit}
          onClear={mapTileCache.clear}
        />
      ) : null}
    </div>
  )
}
