import { Mountain, Palette } from 'lucide-react'
import {
  ColorField,
  NumberField,
  SliderField,
  ToggleField,
} from './widgetFormControls'
import {
  DimensionsSection,
  OpacitySection,
  SectionHeading,
} from './widgetEditorSections'

export default function ElevationWidgetEditor({
  widget,
  updateWidgetData,
  setNumericField,
}) {
  const lineWidth =
    widget.data.completed_line_width ?? widget.data.remaining_line_width ?? 6
  const completedLineOpacity = widget.data.completed_line_opacity ?? 100
  const remainingLineOpacity = widget.data.remaining_line_opacity ?? 35

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title="Profile Styling" />
        <SliderField
          label="Line Thickness"
          value={lineWidth}
          min={0}
          max={20}
          step={1}
          valueDisplay={`${lineWidth}px`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Done Line Color"
            value={widget.data.completed_line_color || '#ffffff'}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
                color: value,
              })
            }
          />
          <ColorField
            label="Remaining Line Color"
            value={widget.data.remaining_line_color || '#71717a'}
            onChange={(value) =>
              updateWidgetData(widget.id, { remaining_line_color: value })
            }
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Done Line Opacity"
            value={completedLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${completedLineOpacity}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { completed_line_opacity: value })
            }
          />
          <SliderField
            label="Remaining Line Opacity"
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { remaining_line_opacity: value })
            }
          />
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Mountain} title="Marker & Labels" />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Marker Size"
            value={widget.data.marker_size ?? 16}
            min={0}
            max={50}
            step={1}
            valueDisplay={`${widget.data.marker_size ?? 16}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_size: value })
            }
          />
          <SliderField
            label="Marker Opacity"
            value={widget.data.marker_opacity ?? 100}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${widget.data.marker_opacity ?? 100}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_opacity: value })
            }
          />
        </div>
        <ColorField
          label="Marker Color"
          value={widget.data.marker_color || '#ffffff'}
          onChange={(value) =>
            updateWidgetData(widget.id, { marker_color: value })
          }
        />
        <ToggleField
          label="Label Metric"
          checked={widget.data.show_elevation_metric ?? true}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_elevation_metric: checked })
          }
        />
        <ToggleField
          label="Label Imperial"
          checked={widget.data.show_elevation_imperial ?? false}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_elevation_imperial: checked })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label="Metric Offset X"
            value={widget.data.metric_label_offset_x ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'metric_label_offset_x', rawValue)
            }
          />
          <NumberField
            label="Metric Offset Y"
            value={widget.data.metric_label_offset_y ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'metric_label_offset_y', rawValue)
            }
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label="Imperial Offset X"
            value={widget.data.imperial_label_offset_x ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'imperial_label_offset_x', rawValue)
            }
          />
          <NumberField
            label="Imperial Offset Y"
            value={widget.data.imperial_label_offset_y ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'imperial_label_offset_y', rawValue)
            }
          />
        </div>
      </div>
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
