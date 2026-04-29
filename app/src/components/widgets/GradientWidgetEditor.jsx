/**
 * Supports widget editing flows related to gradient widget editor.
 */

import { ColorField, SliderField, ToggleField } from './widgetFormControls'
import {
  FontSection,
  OpacitySection,
  SectionHeading,
} from './widgetEditorSections'
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
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        title="Gradient Value"
        fontSizeLabel="Value Font Size"
        colorLabel="Value Color"
      />

      <div className="space-y-4">
        <SectionHeading icon={TrendingUp} title="Gradient Styling" />
        <SliderField
          label="Value Offset"
          value={valueOffset}
          min={-200}
          max={200}
          step={1}
          valueDisplay={`${valueOffset}px`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, { value_offset: value })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Triangle + Color"
            value={widget.data.triangle_positive_color || getThemeColor('aqua')}
            onChange={(value) =>
              updateWidgetData(widget.id, { triangle_positive_color: value })
            }
          />
          <ColorField
            label="Triangle - Color"
            value={
              widget.data.triangle_negative_color || getThemeColor('accent')
            }
            onChange={(value) =>
              updateWidgetData(widget.id, { triangle_negative_color: value })
            }
          />
        </div>
        <ToggleField
          label="Show Sign"
          checked={widget.data.show_sign ?? true}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_sign: checked })
          }
        />
        <ToggleField
          label="Triangle Indicator"
          checked={widget.data.show_triangle ?? true}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_triangle: checked })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Decimals"
            value={decimals}
            min={0}
            max={2}
            step={1}
            valueDisplay={decimals.toString()}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { decimals: value })
            }
          />
          <SliderField
            label="Indicator Width"
            value={triangleWidth}
            min={0}
            max={240}
            step={1}
            valueDisplay={`${triangleWidth}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { triangle_width: value })
            }
          />
        </div>
      </div>
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
