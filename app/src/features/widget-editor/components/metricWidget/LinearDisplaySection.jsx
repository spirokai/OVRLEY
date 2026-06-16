import { SectionHeading } from '../widgetEditorSections'
import { ToggleField, SelectField, SliderField, ColorField } from '../widgetFormControls'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { useMemo } from 'react'
import { SlidersHorizontal, Tags } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'

const ORIENTATION_OPTIONS = [
  { value: 'horizontal', label: 'Horizontal' },
  { value: 'vertical', label: 'Vertical' },
]

/**
 * Renders linear gauge display controls: orientation, track styling,
 * min/max labels.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 */
export default function LinearDisplaySection({ widget, updateWidgetData }) {
  const linearData = useMemo(() => widget.data.display_variants?.linear ?? {}, [widget.data.display_variants?.linear])
  const updateLinear = useDisplayVariantUpdater(widget, 'linear', linearData, updateWidgetData)
  const availableFonts = useAvailableFonts()
  const updateOrientation = (orientation) => {
    if (orientation === linearData.orientation) return
    updateLinear({
      orientation,
      width: linearData.height,
      height: linearData.width,
    })
  }

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={SlidersHorizontal} title="Gauge Track" />
        <SelectField
          label="Orientation"
          value={linearData.orientation}
          onValueChange={updateOrientation}
          options={ORIENTATION_OPTIONS}
        />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Width"
            value={linearData.width}
            min={20}
            max={800}
            step={1}
            valueDisplay={`${linearData.width}px`}
            onSliderChange={(value) => updateLinear({ width: value })}
          />
          <SliderField
            label="Height"
            value={linearData.height}
            min={8}
            max={400}
            step={1}
            valueDisplay={`${linearData.height}px`}
            onSliderChange={(value) => updateLinear({ height: value })}
          />
          <SliderField
            label="Corner Radius"
            value={linearData.track_corner_radius}
            min={0}
            max={40}
            step={1}
            valueDisplay={`${linearData.track_corner_radius}px`}
            onSliderChange={(value) => updateLinear({ track_corner_radius: value })}
          />
          <SliderField
            label="Border"
            value={linearData.track_border_thickness}
            min={0}
            max={12}
            step={1}
            valueDisplay={`${linearData.track_border_thickness}px`}
            onSliderChange={(value) => updateLinear({ track_border_thickness: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField label="Border Color" value={linearData.track_border_color} onChange={(value) => updateLinear({ track_border_color: value })} />
          <ColorField label="Empty Color" value={linearData.track_empty_color} onChange={(value) => updateLinear({ track_empty_color: value })} />
          <ColorField label="Filled Color" value={linearData.track_filled_color} onChange={(value) => updateLinear({ track_filled_color: value })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Empty Opacity"
            value={linearData.track_empty_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(linearData.track_empty_opacity * 100)}%`}
            onSliderChange={(value) => updateLinear({ track_empty_opacity: value })}
          />
          <SliderField
            label="Filled Opacity"
            value={linearData.track_filled_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(linearData.track_filled_opacity * 100)}%`}
            onSliderChange={(value) => updateLinear({ track_filled_opacity: value })}
          />
          <div className="flex items-center justify-between gap-2 px-1 self-end pb-2">
            <span className="text-[9px] text-muted-foreground uppercase font-bold">Flat</span>
            <ToggleField checked={linearData.track_fill_flat} onCheckedChange={(checked) => updateLinear({ track_fill_flat: checked })} />
          </div>
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Tags} title="Min/Max Labels" />
        <FontSelectField
          label="Label Font"
          value={linearData.min_max_label_font}
          onValueChange={(value) => updateLinear({ min_max_label_font: value })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Show Labels</span>
          <ToggleField checked={linearData.show_min_max_labels} onCheckedChange={(checked) => updateLinear({ show_min_max_labels: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Font Size"
            value={linearData.min_max_label_font_size}
            min={6}
            max={32}
            step={1}
            valueDisplay={`${linearData.min_max_label_font_size}px`}
            onSliderChange={(value) => updateLinear({ min_max_label_font_size: value })}
          />
          <ColorField label="Label Color" value={linearData.min_max_label_color} onChange={(value) => updateLinear({ min_max_label_color: value })} />
        </div>
      </div>
    </>
  )
}
