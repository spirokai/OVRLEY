/**
 * Supports widget editing flows related to metric widget editor.
 */

import {
  FontSection,
  IconSection,
  UnitsControlRow,
} from './widgetEditorSections'
import { SPEED_UNITS } from './widgetFormControls'

/**
 * Renders the metric widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
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
              widget={widget}
              updateWidgetData={updateWidgetData}
              title="Unit"
              checked={widget.data.show_units ?? true}
              onCheckedChange={(checked) =>
                updateWidgetData(widget.id, { show_units: checked })
              }
              value={widget.data.speed_unit || 'kmh'}
              onValueChange={(value) =>
                updateWidgetData(widget.id, { speed_unit: value })
              }
              options={SPEED_UNITS}
            />
          ) : (
            <UnitsControlRow
              widget={widget}
              updateWidgetData={updateWidgetData}
              title="Unit"
              checked={widget.data.show_units ?? true}
              onCheckedChange={(checked) =>
                updateWidgetData(widget.id, { show_units: checked })
              }
            />
          )
        }
      />
    </>
  )
}
