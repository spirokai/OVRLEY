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

const POSITION_OPTIONS = [
  { value: 'top', label: 'Top' },
  { value: 'bottom', label: 'Bottom' },
  { value: 'left', label: 'Left' },
  { value: 'right', label: 'Right' },
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
  const availablePositions = useMemo(() => {
    if (linearData.orientation === 'horizontal') {
      return POSITION_OPTIONS.filter((option) => option.value === 'top' || option.value === 'bottom')
    } else {
      return POSITION_OPTIONS.filter((option) => option.value === 'left' || option.value === 'right')
    }
  }, [linearData.orientation])

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={SlidersHorizontal} title="Gauge Track" />
        <div className="grid grid-cols-2 gap-3">
          <SelectField label="Orientation" value={linearData.orientation} onValueChange={updateOrientation} options={ORIENTATION_OPTIONS} />
          <SliderField
            label="Corner Radius"
            value={linearData.track_corner_radius}
            min={0}
            max={40}
            step={1}
            valueDisplay={`${linearData.track_corner_radius}px`}
            onSliderChange={(value) => updateLinear({ track_corner_radius: value })}
          />
        </div>
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
            max={150}
            step={1}
            valueDisplay={`${linearData.height}px`}
            onSliderChange={(value) => updateLinear({ height: value })}
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
          <ColorField label="Border Color" value={linearData.track_border_color} onChange={(value) => updateLinear({ track_border_color: value })} />
        </div>

        <div className="grid grid-cols-2 gap-3">
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
          <div className="flex items-center justify-between gap-2 px-1 self-end pb-2 pt-2">
            <span className="text-[9px] text-muted-foreground uppercase font-bold">Flat</span>
            <ToggleField checked={linearData.track_fill_flat} onCheckedChange={(checked) => updateLinear({ track_fill_flat: checked })} />
          </div>
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex flex-row justify-between items-center gap-2">
          <SectionHeading icon={Tags} title="Min/Max Labels" />
          <ToggleField checked={linearData.show_min_max_labels} onCheckedChange={(checked) => updateLinear({ show_min_max_labels: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <FontSelectField
            label="Label Font"
            value={linearData.min_max_label_font}
            disabled={!linearData.show_min_max_labels}
            onValueChange={(value) => updateLinear({ min_max_label_font: value })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <SliderField
            label="Font Size"
            disabled={!linearData.show_min_max_labels}
            value={linearData.min_max_label_font_size}
            min={6}
            max={50}
            step={1}
            valueDisplay={`${linearData.min_max_label_font_size}px`}
            onSliderChange={(value) => updateLinear({ min_max_label_font_size: value })}
          />
        </div>

        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Label Color"
            value={linearData.min_max_label_color}
            onChange={(value) => updateLinear({ min_max_label_color: value })}
            disabled={!linearData.show_min_max_labels}
          />
          <SelectField
            label="Position"
            disabled={!linearData.show_min_max_labels}
            value={linearData.min_max_label_position}
            onValueChange={(value) => updateLinear({ min_max_label_position: value })}
            options={availablePositions}
          />
        </div>
      </div>
    </>
  )
}
