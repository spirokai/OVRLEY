import { useEffect, useRef, useState } from 'react'
import {
  ColorPicker,
  ColorPickerArea,
  ColorPickerContent,
  ColorPickerEyeDropper,
  ColorPickerHueSlider,
  ColorPickerInput,
  ColorPickerFormatSelect,
  ColorPickerSwatch,
  ColorPickerTrigger,
} from '@/components/ui/color-picker'
import { cn } from '@/lib/utils'
import { normalizeHexColor } from '@/lib/color-utils'

const DEFAULT_PRESET_COLORS = [
  '#ffffff',
  '#afeeee',
  '#40e0d0',
  '#005b5b',
  '#c65102',
  '#000000',
]
const AREA_COMMIT_DEBOUNCE_MS = 0

export default function HexColorPicker({
  value,
  onChange,
  className,
  triggerClassName,
  valueClassName,
  swatchClassName,
  presetColors = DEFAULT_PRESET_COLORS,
  showValue = true,
}) {
  const normalizedValue = normalizeHexColor(value)
  const [open, setOpen] = useState(false)
  const [draftValue, setDraftValue] = useState(normalizedValue)
  const draftValueRef = useRef(normalizedValue)
  const areaDraggingRef = useRef(false)
  const debounceTimeoutRef = useRef(null)

  useEffect(() => {
    if (!open) {
      setDraftValue(normalizedValue)
      draftValueRef.current = normalizedValue
    }
  }, [normalizedValue, open])

  const clearScheduledCommit = () => {
    if (debounceTimeoutRef.current !== null) {
      window.clearTimeout(debounceTimeoutRef.current)
      debounceTimeoutRef.current = null
    }
  }

  const commitDraft = () => {
    clearScheduledCommit()
    const nextValue = normalizeHexColor(draftValueRef.current, normalizedValue)
    if (nextValue !== normalizedValue) {
      onChange(nextValue)
    }
  }

  const scheduleCommit = () => {
    clearScheduledCommit()
    debounceTimeoutRef.current = window.setTimeout(() => {
      debounceTimeoutRef.current = null
      commitDraft()
    }, AREA_COMMIT_DEBOUNCE_MS)
  }

  useEffect(
    () => () => {
      clearScheduledCommit()
    },
    [],
  )

  const displayValue = open ? draftValue : normalizedValue

  return (
    <ColorPicker
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          areaDraggingRef.current = false
          commitDraft()
        }
        setOpen(nextOpen)
      }}
      value={draftValue}
      onValueChange={(nextValue) => {
        const normalizedNextValue = normalizeHexColor(nextValue, draftValue)
        draftValueRef.current = normalizedNextValue
        setDraftValue(normalizedNextValue)

        if (areaDraggingRef.current) {
          scheduleCommit()
        }
      }}
      defaultFormat="hex"
    >
      <div className={cn('flex items-center gap-2', className)}>
        <ColorPickerTrigger asChild>
          <button
            type="button"
            className={cn(
              'flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md border border-border/70 bg-surface px-2 text-left shadow-xs transition-colors hover:border-accent-border hover:bg-surface-highlight-soft',
              triggerClassName,
            )}
          >
            <ColorPickerSwatch
              className={cn(
                'size-5 shrink-0 rounded-[0.4rem] border border-border/70 shadow-sm',
                swatchClassName,
              )}
            />
            {showValue ? (
              <span
                className={cn(
                  'truncate font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground',
                  valueClassName,
                )}
              >
                {displayValue}
              </span>
            ) : null}
          </button>
        </ColorPickerTrigger>

        <ColorPickerContent
          align="start"
          sideOffset={8}
          className="w-[320px] rounded-2xl border-border/80 bg-card/95 p-4 shadow-2xl shadow-background/40 backdrop-blur-sm"
        >
          <div className="space-y-4">
            <ColorPickerArea
              className="h-40 w-full overflow-hidden rounded-sm border border-border/60"
              onPointerDown={() => {
                areaDraggingRef.current = true
              }}
              onPointerUp={() => {
                areaDraggingRef.current = false
                commitDraft()
              }}
            />

            <ColorPickerHueSlider onValueCommit={commitDraft} />

            <div className="flex flex-row justify-between">
              <ColorPickerEyeDropper
                variant="outline"
                size="icon"
                className="h-9 w-9 border-border/70 bg-surface text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
              />
              <ColorPickerFormatSelect className="h-9 w-18 border-border/70 bg-surface font-mono text-xs uppercase" />
              <ColorPickerInput withoutAlpha className="" />
            </div>

            <div className="space-y-2">
              <p className="text-[9px] font-bold uppercase tracking-[0.24em] text-muted-foreground">
                Presets
              </p>
              <div className="grid grid-cols-6 gap-2">
                {presetColors.map((presetColor) => (
                  <button
                    key={presetColor}
                    type="button"
                    className="size-9 rounded-xl border border-border/70 bg-surface shadow-sm transition-transform hover:-translate-y-0.5 hover:border-accent-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    style={{ backgroundColor: presetColor }}
                    onClick={() => {
                      const nextValue = normalizeHexColor(presetColor)
                      draftValueRef.current = nextValue
                      setDraftValue(nextValue)
                      onChange(nextValue)
                    }}
                    aria-label={`Use color ${presetColor}`}
                  />
                ))}
              </div>
            </div>
          </div>
        </ColorPickerContent>
      </div>
    </ColorPicker>
  )
}
