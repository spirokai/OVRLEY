/**
 * Supports widget editing flows related to gradient widget editor.
 */

import { ColorField, SliderField, ToggleField } from './widgetFormControls'
import { Label } from '@/components/ui/label'
import { FontSection, SectionHeading } from './widgetEditorSections'
import { TrendingUp } from 'lucide-react'
import { getThemeColor } from '@/lib/theme'

/**
 * Renders the gradient widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @returns {JSX.Element} Rendered component output.
 */
export default function GradientWidgetEditor({ widget, updateWidgetData }) {
  const valueOffset = widget.data.value_offset ?? 0
  const decimals = widget.data.decimals ?? 0
  const triangleWidth = widget.data.triangle_width ?? 72

  return (
    <>
      <FontSection widget={widget} updateWidgetData={updateWidgetData} title="Typography" fontSizeLabel="Font Size" colorLabel="Value Color" />

      <SliderField
        label="Value Offset"
        value={valueOffset}
        min={-200}
        max={200}
        step={1}
        valueDisplay={`${valueOffset}px`}
        onSliderChange={(value) => updateWidgetData(widget.id, { value_offset: value })}
      />
      <div className="grid grid-cols-2 gap-3">
        <SliderField
          label="Decimals"
          value={decimals}
          min={0}
          max={2}
          step={1}
          valueDisplay={decimals.toString()}
          onSliderChange={(value) => updateWidgetData(widget.id, { decimals: value })}
        />
        <div className="flex items-center justify-between rounded-md pl-8 py-2.5 mt-4.5">
          <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Show sign</Label>
          <ToggleField checked={widget.data.show_sign ?? true} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_sign: checked })} />
        </div>
      </div>
      <div className="space-y-4">
        <div className="flex w-full justify-between items-center">
          <SectionHeading icon={TrendingUp} title="Indicator" />
          <ToggleField
            checked={widget.data.show_triangle ?? true}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_triangle: checked })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Color Positive"
            disabled={!widget.data.show_triangle}
            value={widget.data.triangle_positive_color || getThemeColor('aqua')}
            onChange={(value) => updateWidgetData(widget.id, { triangle_positive_color: value })}
          />
          <ColorField
            label="Color Negative"
            disabled={!widget.data.show_triangle}
            value={widget.data.triangle_negative_color || getThemeColor('accent')}
            onChange={(value) => updateWidgetData(widget.id, { triangle_negative_color: value })}
          />
        </div>

        <SliderField
          label="Width"
          disabled={!widget.data.show_triangle}
          value={triangleWidth}
          min={0}
          max={240}
          step={1}
          valueDisplay={`${triangleWidth}px`}
          onSliderChange={(value) => updateWidgetData(widget.id, { triangle_width: value })}
        />
      </div>
    </>
  )
}
