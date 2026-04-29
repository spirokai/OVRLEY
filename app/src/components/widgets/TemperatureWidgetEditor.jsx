/**
 * Supports widget editing flows related to temperature widget editor.
 */

import { TEMPERATURE_UNITS, UnitsControlRow } from './widgetFormControls'
import {
  FontSection,
  IconSection,
  OpacitySection,
} from './widgetEditorSections'

/**
 * Renders the temperature widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
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
