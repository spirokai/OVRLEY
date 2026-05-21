/**
 * Shared form control components used across widget editors.
 * Each component is a thin wrapper around a shadcn/ui primitive with consistent widget-editor styling.
 */

/* eslint-disable react-refresh/only-export-components */

import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import HexColorPicker from '@/components/ui/hex-color-picker'
import { RECOMMENDED_FONTS } from '@/lib/fonts'
import { cn } from '@/lib/utils'

export const FONTS = RECOMMENDED_FONTS

export const TIME_FORMATS = [
  { value: 'date-dd-mm-yyyy', label: 'Date only (DD-MM-YYYY)' },
  { value: 'date-mm-dd-yyyy', label: 'Date only (MM-DD-YYYY)' },
  { value: 'date-yyyy-mm-dd', label: 'Date only (YYYY-MM-DD)' },
  { value: 'date-dd-mmm-yyyy', label: 'Date only (DD MMM YYYY)' },
  { value: 'date-mmm-dd-yyyy', label: 'Date only (MMM DD YYYY)' },
  { value: 'date-dd-mmmm-yyyy', label: 'Date only (DD MMMM YYYY)' },
  { value: 'date-mmmm-dd-yyyy', label: 'Date only (MMMM DD YYYY)' },
  { value: 'time-24', label: 'Time only (24h)' },
  { value: 'time-12', label: 'Time only (12h)' },
  { value: 'date-time-24', label: 'Date + time (24h)' },
  { value: 'date-time-12', label: 'Date + time (12h)' },
  { value: 'date-mmm-time-24', label: 'Date + time (DD MMM, 24h)' },
  { value: 'date-mmm-time-12', label: 'Date + time (DD MMM, 12h)' },
  { value: 'date-mmmm-time-24', label: 'Date + time (DD MMMM, 24h)' },
  { value: 'date-mmmm-time-12', label: 'Date + time (DD MMMM, 12h)' },
]

export const SPEED_UNITS = [
  { value: 'kmh', label: 'km/h' },
  { value: 'mph', label: 'mph' },
  { value: 'kn', label: 'kn' },
  { value: 'mps', label: 'm/s' },
]

export const TEMPERATURE_UNITS = [
  { value: 'celsius', label: '\u00B0C' },
  { value: 'fahrenheit', label: '\u00B0F' },
]

export const CONTROL_CLASS = 'h-9 border-border/70 bg-surface text-xs'
const FIELD_LABEL_CLASS = 'text-[9px] text-muted-foreground uppercase font-bold'

/**
 * Renders the field block component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.children - Nested React children.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
export function FieldBlock({ label, children, className, disabled = false }) {
  return (
    <div className={cn('space-y-1', className)}>
      <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
        {label}
      </Label>
      {children}
    </div>
  )
}

/**
 * Renders the select field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onValueChange - Callback invoked to value change.
 * @param {*} props.options - Configuration options for the helper.
 * @param {*} props.disabled - Value for disabled.
 * @returns {JSX.Element} Rendered component output.
 */
export function SelectField({ label, value, onValueChange, options, disabled = false }) {
  return (
    <FieldBlock label={label}>
      <Select value={value} onValueChange={onValueChange} disabled={disabled}>
        <SelectTrigger className={cn(CONTROL_CLASS, disabled && 'opacity-50 pointer-events-none')}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </FieldBlock>
  )
}

/**
 * Renders the text field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onChange - Callback invoked to change.
 * @param {*} props.placeholder - Value for placeholder.
 * @returns {JSX.Element} Rendered component output.
 */
export function TextField({ label, value, onChange, placeholder = '' }) {
  return (
    <FieldBlock label={label}>
      <BlurInput value={value} onChange={(event) => onChange(event.target.value)} className={CONTROL_CLASS} placeholder={placeholder} />
    </FieldBlock>
  )
}

/**
 * Renders the number field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onChange - Callback invoked to change.
 * @param {*} props.min - Lower bound used by the calculation.
 * @param {*} props.max - Upper bound used by the calculation.
 * @param {*} props.step - Value for step.
 * @returns {JSX.Element} Rendered component output.
 */
export function NumberField({ label, value, onChange, min, max, disabled = false, step = 1 }) {
  return (
    <FieldBlock label={label} disabled={disabled}>
      <BlurInput
        type="number"
        disabled={disabled}
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(event) => onChange(event.target.value)}
        className={CONTROL_CLASS}
      />
    </FieldBlock>
  )
}

/**
 * Renders the color field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onChange - Callback invoked to change.
 * @returns {JSX.Element} Rendered component output.
 */
export function ColorField({ label, value, onChange, disabled = false }) {
  return (
    <div className="space-y-2">
      <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
        {label}
      </Label>
      <HexColorPicker value={value} onChange={onChange} disabled={disabled} triggerClassName="justify-start" />
    </div>
  )
}

/**
 * Renders the slider field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.min - Lower bound used by the calculation.
 * @param {*} props.max - Upper bound used by the calculation.
 * @param {*} props.step - Value for step.
 * @param {*} props.onSliderChange - Callback invoked to slider change.
 * @param {*} props.valueDisplay - Value for value display.
 * @returns {JSX.Element} Rendered component output.
 */
export function SliderField({ label, value, min, max, step = 1, disabled = false, onSliderChange, valueDisplay }) {
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
          {label}
        </Label>
        <span className="text-[10px] font-mono text-muted-foreground">{valueDisplay}</span>
      </div>
      <div className="flex items-center gap-3 px-1">
        <Slider
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          value={[value]}
          onValueChange={([nextValue]) => onSliderChange(nextValue)}
          className="flex-1 py-2"
        />
      </div>
    </div>
  )
}

/**
 * Renders the toggle field component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.checked - Value for checked.
 * @param {*} props.onCheckedChange - Callback invoked to checked change.
 * @param {*} props.className - Additional class names to merge into the element.
 * @returns {JSX.Element} Rendered component output.
 */
export function ToggleField({ checked, onCheckedChange, className }) {
  return <Switch size="xs" checked={checked} onCheckedChange={onCheckedChange} className={className} />
}
