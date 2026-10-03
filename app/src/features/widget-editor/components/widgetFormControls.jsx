/**
 * Shared form control components used across widget editors.
 * Each component is a thin wrapper around a shadcn/ui primitive with consistent widget-editor styling.
 */

/* eslint-disable react-refresh/only-export-components */

import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Input } from '@/components/ui/input'
import { Slider } from '@/components/ui/slider'
import { useInlineNumberInput } from '@/hooks/useInlineNumberInput'
import { Switch } from '@/components/ui/switch'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Button } from '@/components/ui/button'
import HexColorPicker from '@/components/ui/hex-color-picker'
import { cn } from '@/lib/utils'
import { AlignCenter, AlignLeft, AlignRight, RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export const TIME_FORMATS = [
  { value: 'date-dd-mm-yyyy', labelKey: 'widget-editor.dateFormatDdMmYyyy', defaultLabel: 'Date only (DD-MM-YYYY)' },
  { value: 'date-mm-dd-yyyy', labelKey: 'widget-editor.dateFormatMmDdYyyy', defaultLabel: 'Date only (MM-DD-YYYY)' },
  { value: 'date-yyyy-mm-dd', labelKey: 'widget-editor.dateFormatYyyyMmDd', defaultLabel: 'Date only (YYYY-MM-DD)' },
  { value: 'date-dd-mmm-yyyy', labelKey: 'widget-editor.dateFormatDdMmmYyyy', defaultLabel: 'Date only (DD MMM YYYY)' },
  { value: 'date-mmm-dd-yyyy', labelKey: 'widget-editor.dateFormatMmmDdYyyy', defaultLabel: 'Date only (MMM DD YYYY)' },
  { value: 'date-dd-mmmm-yyyy', labelKey: 'widget-editor.dateFormatDdMmmmYyyy', defaultLabel: 'Date only (DD MMMM YYYY)' },
  { value: 'date-mmmm-dd-yyyy', labelKey: 'widget-editor.dateFormatMmmmDdYyyy', defaultLabel: 'Date only (MMMM DD YYYY)' },
  { value: 'time-24', labelKey: 'widget-editor.timeFormat24h', defaultLabel: 'Time only (24h)' },
  { value: 'time-24s', labelKey: 'widget-editor.timeFormat24hWithSeconds', defaultLabel: 'Time only (24h with seconds)' },
  { value: 'time-12', labelKey: 'widget-editor.timeFormat12h', defaultLabel: 'Time only (12h)' },
  { value: 'time-12s', labelKey: 'widget-editor.timeFormat12hWithSeconds', defaultLabel: 'Time only (12h with seconds)' },
  { value: 'date-time-24', labelKey: 'widget-editor.dateTimeFormat24h', defaultLabel: 'Date + time (24h)' },
  { value: 'date-time-24s', labelKey: 'widget-editor.dateTimeFormat24hWithSeconds', defaultLabel: 'Date + time (24h with seconds)' },
  { value: 'date-time-12', labelKey: 'widget-editor.dateTimeFormat12h', defaultLabel: 'Date + time (12h)' },
  { value: 'date-time-12s', labelKey: 'widget-editor.dateTimeFormat12hWithSeconds', defaultLabel: 'Date + time (12h with seconds)' },
  { value: 'date-mmm-time-24', labelKey: 'widget-editor.dateTimeFormatDdMmm24h', defaultLabel: 'Date + time (DD MMM, 24h)' },
  { value: 'date-mmm-time-12', labelKey: 'widget-editor.dateTimeFormatDdMmm12h', defaultLabel: 'Date + time (DD MMM, 12h)' },
  { value: 'date-mmmm-time-24', labelKey: 'widget-editor.dateTimeFormatDdMmmm24h', defaultLabel: 'Date + time (DD MMMM, 24h)' },
  { value: 'date-mmmm-time-12', labelKey: 'widget-editor.dateTimeFormatDdMmmm12h', defaultLabel: 'Date + time (DD MMMM, 12h)' },
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

const CONTENT_ALIGNMENT_OPTIONS = [
  { value: 'left', labelKey: 'widget-editor.alignLeft', defaultLabel: 'Align left', icon: AlignLeft },
  { value: 'center', labelKey: 'widget-editor.alignCenter', defaultLabel: 'Align center', icon: AlignCenter },
  { value: 'right', labelKey: 'widget-editor.alignRight', defaultLabel: 'Align right', icon: AlignRight },
]

export const CONTROL_CLASS = 'h-9 border-border/70 bg-surface text-xs'
const FIELD_LABEL_CLASS = 'h-3 text-[9px] text-muted-foreground uppercase font-bold'

/**
 * Renders the alignment selector for intrinsic value widgets.
 *
 * @param {object} props
 * @param {'left'|'center'|'right'} props.value - Current alignment.
 * @param {Function} props.onValueChange - Commits a non-empty alignment selection.
 * @returns {JSX.Element}
 */
export function ContentAlignmentControl({ value, onValueChange }) {
  const { t } = useTranslation()

  return (
    <div className="flex items-center">
      <ToggleGroup
        size="compact"
        type="single"
        value={value}
        aria-label={t('widget-editor.contentAlignment', 'Content Alignment')}
        onValueChange={(nextValue) => {
          if (nextValue) onValueChange(nextValue)
        }}
      >
        {CONTENT_ALIGNMENT_OPTIONS.map((option) => {
          const Icon = option.icon
          const label = t(option.labelKey, option.defaultLabel)
          return (
            <ToggleGroupItem key={option.value} value={option.value} size="compact" aria-label={label} title={label}>
              <Icon />
            </ToggleGroupItem>
          )
        })}
      </ToggleGroup>
    </div>
  )
}

/**
 * Renders the field block component.
 *
 * @param {object} props - Component props.
 * @param {*} props.label - Field or UI label text.
 * @param {*} props.children - Nested React children.
 * @param {*} props.className - Additional class names to merge into the element.
 * @param {Function} props.onReset - Optional callback that enables the reset action.
 * @returns {JSX.Element} Rendered component output.
 */
export function FieldBlock({ label, children, className, disabled = false, onReset }) {
  const { t } = useTranslation()
  return (
    <div className={cn('space-y-1', className)}>
      <div className="relative h-3">
        <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
          {label}
        </Label>
        {onReset ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="absolute -top-1 right-0 h-5 w-5 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            onClick={onReset}
            aria-label={label ? t('widget-editor.resetLabel', 'Reset {{label}}', { label }) : t('widget-editor.resetField', 'Reset field')}
          >
            <RotateCcw className="size-3.5" />
          </Button>
        ) : null}
      </div>
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
 * @param {object} props.contentProps - Positioning options passed to the select menu.
 * @param {Function} props.onReset - Optional callback that enables the reset action.
 * @returns {JSX.Element} Rendered component output.
 */
export function SelectField({ label, value, onValueChange, options, disabled = false, contentProps, onReset }) {
  return (
    <FieldBlock label={label} disabled={disabled} onReset={onReset}>
      <Select value={value} onValueChange={onValueChange} disabled={disabled}>
        <SelectTrigger className={cn(CONTROL_CLASS, disabled && 'opacity-50 pointer-events-none')}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent {...contentProps}>
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
 * @param {*} props.placeholder - Optional empty-state hint.
 * @param {Function} props.onReset - Optional callback that enables the reset action.
 * @returns {JSX.Element} Rendered component output.
 */
export function NumberField({ label, value, onChange, min, max, disabled = false, step = 1, suffix, integerDisplay = true, placeholder, onReset }) {
  return (
    <FieldBlock label={label} disabled={disabled} onReset={onReset}>
      <div className="relative">
        <BlurInput
          type="number"
          disabled={disabled}
          value={value === null || value === undefined || value === '' || !integerDisplay ? value : Math.round(value)}
          min={min}
          max={max}
          step={step}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
          className={cn(CONTROL_CLASS, suffix && 'pr-16')}
        />
        {suffix ? (
          <span className="pointer-events-none absolute inset-y-0 right-8 flex items-center text-[10px] font-mono text-muted-foreground">
            {suffix}
          </span>
        ) : null}
      </div>
    </FieldBlock>
  )
}

/**
 * Renders the color field component.
 *
 * @param {object} props - Component props.
 * @param {string} [props.label] - Explicit field label.
 * @param {string} [props.labelKey='widget-editor.color'] - Translation key used when label is absent.
 * @param {string} [props.defaultLabel='Color'] - Default translation used when label is absent.
 * @param {*} props.value - Input value processed by the helper.
 * @param {*} props.onChange - Callback invoked to change.
 * @returns {JSX.Element} Rendered component output.
 */
export function ColorField({ label, labelKey = 'widget-editor.color', defaultLabel = 'Color', value, onChange, disabled = false }) {
  const { t } = useTranslation()
  const resolvedLabel = label ?? t(labelKey, defaultLabel)

  return (
    <div className="space-y-1">
      <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
        {resolvedLabel}
      </Label>
      <HexColorPicker value={value} onChange={onChange} disabled={disabled} triggerClassName="justify-start" />
    </div>
  )
}

// A separate child keeps draft keystrokes from rerendering the slider.
function SliderValueInput({ valueDisplay, valueScale, value, min, max, step, disabled, onSliderChange, onSliderCommit, ...inputProps }) {
  const draftProps = useInlineNumberInput({
    value: Number((value * valueScale).toPrecision(12)),
    onCommit: (displayValue) => {
      const nextValue = displayValue / valueScale
      onSliderChange(nextValue)
      onSliderCommit?.(nextValue)
    },
  })

  return (
    <div className="group relative flex h-3.75 min-w-[5ch] shrink-0 items-center justify-end text-right text-[10px] leading-none font-mono text-muted-foreground">
      <span
        aria-hidden="true"
        className={cn('pointer-events-none group-focus-within:invisible group-has-aria-invalid:invisible', disabled && 'opacity-50')}
      >
        {valueDisplay}
      </span>
      <Input
        {...inputProps}
        {...draftProps}
        variant="inline"
        type="number"
        min={min * valueScale}
        max={max * valueScale}
        step={step * valueScale}
        disabled={disabled}
        required
        className="absolute inset-x-0 top-1/2 -translate-y-1/2 opacity-0 focus:opacity-100 aria-invalid:opacity-100 disabled:opacity-0"
      />
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
 * @param {'ltr'|'rtl'} [props.dir='ltr'] - Slider direction.
 * @param {boolean} props.integerDisplay - Whether to round the displayed value.
 * @param {*} props.onSliderChange - Callback invoked to slider change.
 * @param {*} props.onSliderCommit - Callback invoked when slider interaction ends.
 * @param {*} props.valueDisplay - Value for value display.
 * @param {string} [props.suffix=''] - Unit shown in the inactive readout.
 * @param {boolean} [props.editable=false] - Enables editing the readout in scene settings and widget editors.
 * @param {number} [props.valueScale=1] - Conversion from slider units into displayed input units.
 * @returns {JSX.Element} Rendered component output.
 */
export function SliderField({
  label,
  value,
  min,
  max,
  step = 1,
  dir = 'ltr',
  disabled = false,
  onSliderChange,
  onSliderCommit,
  valueDisplay,
  suffix = '',
  integerDisplay = false,
  editable = false,
  valueScale = 1,
}) {
  const displayValue = (integerDisplay ? String(Math.round(value)) : valueDisplay) + suffix

  return (
    <div className="space-y-3">
      <div className={cn('flex justify-between gap-3', editable ? 'h-3.75 items-baseline' : 'items-center')}>
        <Label className={FIELD_LABEL_CLASS} disabled={disabled}>
          {label}
        </Label>
        {editable ? (
          <SliderValueInput
            aria-label={label}
            value={value}
            valueDisplay={displayValue}
            valueScale={valueScale}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            onSliderChange={onSliderChange}
            onSliderCommit={onSliderCommit}
          />
        ) : (
          <span className={cn('text-[10px] font-mono text-muted-foreground', disabled && 'opacity-50')}>{displayValue}</span>
        )}
      </div>
      <div className="flex items-center gap-3 px-1">
        <Slider
          dir={dir}
          aria-label={label}
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          value={[value]}
          onValueChange={([nextValue]) => onSliderChange(nextValue)}
          onValueCommit={onSliderCommit ? ([nextValue]) => onSliderCommit(nextValue) : undefined}
          className="flex-1 py-2"
        />
      </div>
    </div>
  )
}

/**
 * Renders a size slider with a separate interaction commit callback.
 *
 * @param {object} props - Slider props.
 * @param {Function} props.onChange - Live size callback.
 * @param {Function} [props.onCommit] - Size commit callback.
 * @returns {JSX.Element} Rendered size slider.
 */
export function SizeSlider({ onChange, onCommit, ...props }) {
  return <SliderField {...props} editable integerDisplay onSliderChange={onChange} onSliderCommit={onCommit} />
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
