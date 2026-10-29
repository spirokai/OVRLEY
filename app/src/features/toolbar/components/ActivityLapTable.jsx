import { Switch } from '@/components/ui/switch'
import { useTranslation } from 'react-i18next'

/**
 * Renders the detected laps with a switch that includes or excludes each lap from lap widgets.
 *
 * @param {object} props - Component props.
 * @param {Array<{id: number, number: string, duration: string, start: string, enabled: boolean}>} props.rows - Lap table rows.
 * @param {(lapId: number, enabled: boolean) => void} props.onLapEnabledChange - Includes or excludes a lap.
 * @returns {JSX.Element} Rendered lap table.
 */
export function ActivityLapTable({ rows, onLapEnabledChange }) {
  const { t } = useTranslation()

  return (
    <table className="w-full table-fixed text-xs">
      <thead>
        <tr className="text-left font-bold text-muted-foreground">
          <th className="w-10 px-2 pb-1.5 font-bold">{t('toolbar.lapNumber', 'Lap')}</th>
          <th className="px-2 pb-1.5 font-bold">{t('toolbar.lapDuration', 'Duration')}</th>
          <th className="px-2 pb-1.5 font-bold">{t('toolbar.lapStart', 'Start')}</th>
          <th className="w-12 px-2 pb-1.5">
            <span className="sr-only">{t('toolbar.lapShown', 'Shown')}</span>
          </th>
        </tr>
      </thead>
      <tbody className="font-medium tabular-nums text-foreground/90">
        {rows.map((row) => (
          <tr key={row.id} className={row.enabled ? undefined : 'text-muted-foreground/70'}>
            <td className="px-2 py-1">{row.number}</td>
            <td className="px-2 py-1">{row.duration}</td>
            <td className="px-2 py-1">{row.start}</td>
            <td className="px-2 py-1 text-right">
              <Switch
                aria-label={t('toolbar.showLap', { defaultValue: 'Show lap {{lap}}', lap: row.number })}
                checked={row.enabled}
                onCheckedChange={(checked) => onLapEnabledChange(row.id, checked)}
              />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
