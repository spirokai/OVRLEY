import { Map, Palette } from 'lucide-react'
import { ColorField, NumberField, SliderField } from './widgetFormControls'
import {
  DimensionsSection,
  OpacitySection,
  SectionHeading,
} from './widgetEditorSections'

export default function RouteMapWidgetEditor({
  widget,
  updateWidgetData,
  setNumericField,
}) {
  const lineWidth =
    widget.data.completed_line_width ?? widget.data.remaining_line_width ?? 6
  const completedLineOpacity = widget.data.completed_line_opacity ?? 100
  const remainingLineOpacity = widget.data.remaining_line_opacity ?? 35
  const markerSize = widget.data.marker_size ?? 18
  const markerOpacity = widget.data.marker_opacity ?? 100
  const rotation = widget.data.rotation ?? 0

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title="Path Styling" />
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
        <SectionHeading icon={Map} title="Marker & Rotation" />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Marker Size"
            value={markerSize}
            min={0}
            max={50}
            step={1}
            valueDisplay={`${markerSize}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_size: value })
            }
          />
          <SliderField
            label="Marker Opacity"
            value={markerOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${markerOpacity}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_opacity: value })
            }
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Marker Color"
            value={widget.data.marker_color || '#ffffff'}
            onChange={(value) =>
              updateWidgetData(widget.id, { marker_color: value })
            }
          />
          <NumberField
            label="Map Rotation"
            value={rotation}
            min={0}
            max={360}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'rotation', rawValue, {
                min: 0,
                max: 360,
              })
            }
          />
        </div>
      </div>
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
