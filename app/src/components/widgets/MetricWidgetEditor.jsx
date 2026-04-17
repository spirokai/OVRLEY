import { UnitsControlRow, SPEED_UNITS } from './widgetFormControls'
import {
  FontSection,
  IconSection,
  OpacitySection,
} from './widgetEditorSections'

export default function MetricWidgetEditor({
  widget,
  updateWidgetData,
  setNumericField,
}) {
  return (
    <>
      <FontSection widget={widget} updateWidgetData={updateWidgetData} />
      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
        showUnitsToggle
        unitsField={
          widget.type === 'speed' ? (
            <UnitsControlRow
              label="Display Units"
              checked={widget.data.show_units ?? true}
              onCheckedChange={(checked) =>
                updateWidgetData(widget.id, { show_units: checked })
              }
              selectLabel="Speed Units"
              value={widget.data.speed_unit || 'kmh'}
              onValueChange={(value) =>
                updateWidgetData(widget.id, { speed_unit: value })
              }
              options={SPEED_UNITS}
            />
          ) : null
        }
      />
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
