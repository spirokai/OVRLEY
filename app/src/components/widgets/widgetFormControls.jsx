import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'

export const FONTS = [
  { id: 'Arial.ttf', name: 'Arial' },
  { id: 'Evogria.otf', name: 'Evogria' },
  { id: 'Furore.otf', name: 'Furore' },
]

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
  { value: 'celsius', label: 'C' },
  { value: 'fahrenheit', label: 'F' },
]

export const CONTROL_CLASS = 'h-9 border-border/70 bg-surface text-xs'
const COLOR_PICKER_CLASS =
  'h-9 w-11 cursor-pointer rounded-md border border-border/70 bg-surface-strong p-1'
const FIELD_LABEL_CLASS = 'text-[9px] text-muted-foreground uppercase font-bold'

export function FieldBlock({ label, children, className }) {
  return (
    <div className={cn('space-y-1', className)}>
      <Label className={FIELD_LABEL_CLASS}>{label}</Label>
      {children}
    </div>
  )
}

export function SelectField({
  label,
  value,
  onValueChange,
  options,
  disabled = false,
}) {
  return (
    <FieldBlock label={label}>
      <Select value={value} onValueChange={onValueChange} disabled={disabled}>
        <SelectTrigger
          className={cn(
            CONTROL_CLASS,
            disabled && 'opacity-50 pointer-events-none',
          )}
        >
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

export function TextField({ label, value, onChange, placeholder = '' }) {
  return (
    <FieldBlock label={label}>
      <BlurInput
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={CONTROL_CLASS}
        placeholder={placeholder}
      />
    </FieldBlock>
  )
}

export function NumberField({ label, value, onChange, min, max, step = 1 }) {
  return (
    <FieldBlock label={label}>
      <BlurInput
        type="number"
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

export function ColorField({ label, value, onChange }) {
  return (
    <div className="space-y-2">
      <Label className={FIELD_LABEL_CLASS}>{label}</Label>
      <div className="flex gap-2">
        <Input
          type="color"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className={COLOR_PICKER_CLASS}
        />
        <BlurInput
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className={cn(CONTROL_CLASS, 'flex-1 font-mono')}
        />
      </div>
    </div>
  )
}

export function SliderField({
  label,
  value,
  min,
  max,
  step = 1,
  onSliderChange,
  valueDisplay,
}) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <Label className={FIELD_LABEL_CLASS}>{label}</Label>
        <span className="text-[10px] font-mono text-muted-foreground">
          {valueDisplay}
        </span>
      </div>
      <div className="flex items-center gap-3">
        <Slider
          min={min}
          max={max}
          step={step}
          value={[value]}
          onValueChange={([nextValue]) => onSliderChange(nextValue)}
          className="flex-1 py-2"
        />
      </div>
    </div>
  )
}

export function ToggleField({ label, checked, onCheckedChange }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-surface/80 px-3 py-2">
      <p className="text-xs font-medium text-foreground">{label}</p>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}

export function UnitsControlRow({
  checked,
  onCheckedChange,
  label,
  value,
  onValueChange,
  options,
  selectLabel,
}) {
  return (
    <div className="grid grid-cols-2 gap-3 items-end">
      <div className="flex h-9 items-center justify-between gap-3 rounded-md border border-border/60 bg-surface/80 px-3">
        <p className="text-xs font-medium text-foreground">{label}</p>
        <Switch checked={checked} onCheckedChange={onCheckedChange} />
      </div>
      <SelectField
        label={selectLabel}
        value={value}
        onValueChange={onValueChange}
        options={options}
        disabled={!checked}
      />
    </div>
  )
}
