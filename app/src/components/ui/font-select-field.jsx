/**
 * Provides reusable font select field UI primitives for the application.
 */

import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { getFontSelectOptions } from '@/lib/fonts'
import { useTranslation } from 'react-i18next'

/**
 * Renders the font select field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onValueChange - Callback invoked to value change.
 * @param {*} props.recommendedFonts - Value for bundled/recommended fonts.
 * @param {*} props.systemFonts - Value for system fonts.
 * @param {*} props.triggerClassName - Value for trigger class name.
 * @param {*} props.labelClassName - Value for label class name.
 * @returns {JSX.Element} Rendered component output.
 */
export default function FontSelectField({
  label,
  value,
  onValueChange,
  disabled = false,
  recommendedFonts = [],
  systemFonts = [],
  triggerClassName = 'h-8 text-xs',
  labelClassName = 'text-[10px] text-muted-foreground uppercase font-bold',
}) {
  const { t } = useTranslation()
  const { recommendedOptions, filteredSystemFonts } = getFontSelectOptions(value, recommendedFonts, systemFonts)

  return (
    <div className="space-y-2">
      <Label className={labelClassName} disabled={disabled}>
        {label}
      </Label>
      <Select value={value} onValueChange={onValueChange}>
        <SelectTrigger className={triggerClassName} disabled={disabled}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>{t('components.recommended', 'Recommended')}</SelectLabel>
            {recommendedOptions.map((font) => (
              <SelectItem key={font.id} value={font.id}>
                {font.name}
              </SelectItem>
            ))}
          </SelectGroup>
          {filteredSystemFonts.length ? (
            <>
              <SelectSeparator />
              <SelectGroup>
                <SelectLabel>{t('components.system', 'System')}</SelectLabel>
                {filteredSystemFonts.map((font) => (
                  <SelectItem key={font.id} value={font.id}>
                    {font.name}
                  </SelectItem>
                ))}
              </SelectGroup>
            </>
          ) : null}
        </SelectContent>
      </Select>
    </div>
  )
}
