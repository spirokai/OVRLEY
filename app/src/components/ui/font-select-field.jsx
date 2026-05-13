/**
 * Provides reusable font select field UI primitives for the application.
 */

import { useMemo } from 'react'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { formatFontLabel, normalizeFontKey, RECOMMENDED_FONTS } from '@/lib/fonts'

/**
 * Renders the font select field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onValueChange - Callback invoked to value change.
 * @param {*} props.systemFonts - Value for system fonts.
 * @param {*} props.triggerClassName - Value for trigger class name.
 * @param {*} props.labelClassName - Value for label class name.
 * @returns {JSX.Element} Rendered component output.
 */
export default function FontSelectField({
  label,
  value,
  onValueChange,
  systemFonts,
  triggerClassName = 'h-8 text-xs',
  labelClassName = 'text-[10px] text-muted-foreground uppercase font-bold',
}) {
  const systemFontOptions = useMemo(() => systemFonts.map((fontName) => ({ id: fontName, name: fontName })), [systemFonts])
  const recommendedNameKeys = useMemo(() => new Set(RECOMMENDED_FONTS.map((font) => normalizeFontKey(font.name))), [])
  const currentValueKey = normalizeFontKey(value)

  const hasKnownCurrentValue =
    !currentValueKey ||
    RECOMMENDED_FONTS.some((font) => normalizeFontKey(font.id) === currentValueKey || normalizeFontKey(font.name) === currentValueKey) ||
    systemFontOptions.some((font) => normalizeFontKey(font.id) === currentValueKey || normalizeFontKey(font.name) === currentValueKey)

  const recommendedOptions = hasKnownCurrentValue ? RECOMMENDED_FONTS : [{ id: value, name: formatFontLabel(value) }, ...RECOMMENDED_FONTS]

  const filteredSystemFonts = systemFontOptions.filter((font) => !recommendedNameKeys.has(normalizeFontKey(font.name)))

  return (
    <div className="space-y-2">
      <Label className={labelClassName}>{label}</Label>
      <Select value={value} onValueChange={onValueChange}>
        <SelectTrigger className={triggerClassName}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>Recommended</SelectLabel>
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
                <SelectLabel>System</SelectLabel>
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
