/**
 * Supports widget editing flows related to gradient widget editor.
 */

import { ColorField, SizeSlider, SliderField, ToggleField } from './widgetFormControls'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { FontSection, UnitsControlRow } from './widgetEditorSections'
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
export default function GradientWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const valueOffset = widget.data.value_offset
  const decimals = widget.data.decimals
  const triangleWidth = widget.data.triangle_width

  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        title="Typography"
        fontSizeLabel="Font Size"
        colorLabel="Value Color"
      />

      <SliderField
        label="Value Offset"
        value={valueOffset}
        min={-200}
        max={200}
        step={1}
        integerDisplay
        valueDisplay={`${valueOffset}px`}
        onSliderChange={(value) => updateWidgetSize(widget.id, { value_offset: value })}
        onSliderCommit={() => commitWidgetSize(widget.id)}
      />
      <div className="grid grid-cols-2 gap-4">
        <SliderField
          label="Decimals"
          value={decimals}
          min={0}
          max={2}
          step={1}
          valueDisplay={decimals.toString()}
          onSliderChange={(value) => updateWidgetData(widget.id, { decimals: value })}
        />
        <div className="flex items-center justify-between rounded-sm pl-8 py-2.5 mt-4.5">
          <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Show sign</Label>
          <ToggleField checked={widget.data.show_sign} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_sign: checked })} />
        </div>
      </div>
      <div className="space-y-4">
        <div className="flex w-full items-center gap-3">
          <SectionHeading icon={TrendingUp} title="Indicator" />
          <div className="shrink-0 pt-1">
            <ToggleField checked={widget.data.show_triangle} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_triangle: checked })} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
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

        <SizeSlider
          label="Width"
          disabled={!widget.data.show_triangle}
          value={triangleWidth}
          min={0}
          max={240}
          step={1}
          valueDisplay={`${triangleWidth}px`}
          onChange={(value) => updateWidgetSize(widget.id, { triangle_width: value })}
          onCommit={() => commitWidgetSize(widget.id)}
        />
        <UnitsControlRow
          widget={widget}
          updateWidgetData={updateWidgetData}
          title="Unit"
          showToggle={false}
          colorLabel="Percent Color"
          colorValue={widget.data.unit_color}
          onColorChange={(value) => updateWidgetData(widget.id, { unit_color: value })}
        />
      </div>
    </>
  )
}
