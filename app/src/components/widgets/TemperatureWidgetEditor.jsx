import { TEMPERATURE_UNITS, UnitsControlRow } from './widgetFormControls'
import {
  FontSection,
  IconSection,
  OpacitySection,
} from './widgetEditorSections'

export default function TemperatureWidgetEditor({
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
          <UnitsControlRow
            label="Display Units"
            checked={widget.data.show_units ?? true}
            onCheckedChange={(checked) =>
              updateWidgetData(widget.id, { show_units: checked })
            }
            selectLabel="Units"
            value={widget.data.temperature_unit || 'celsius'}
            onValueChange={(value) =>
              updateWidgetData(widget.id, { temperature_unit: value })
            }
            options={TEMPERATURE_UNITS}
          />
        }
      />
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
