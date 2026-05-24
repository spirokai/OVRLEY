/**
 * Supports widget editing flows related to metric widget editor.
 */

import { getStandardMetricDefinition, getStandardMetricDisplayUnit, getStandardMetricUnitOptions } from '@/lib/standard-metrics'
import { FontSection, IconSection, UnitsControlRow } from './widgetEditorSections'

/**
 * Renders the metric widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function MetricWidgetEditor({ widget, updateWidgetData, setNumericField }) {
  const definition = getStandardMetricDefinition(widget.type)
  const unitOptions = getStandardMetricUnitOptions(widget.type)
  const showUnits = widget.data.show_units ?? definition?.showUnitsByDefault ?? false
  const supportsUnitSelection = unitOptions.length > 1

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
            widget={widget}
            updateWidgetData={updateWidgetData}
            title={supportsUnitSelection ? 'Units' : 'Unit'}
            checked={showUnits}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_units: checked })}
            colorValue={widget.data.unit_color || '#ffffff'}
            onColorChange={(value) => updateWidgetData(widget.id, { unit_color: value })}
            selectLabel="Unit"
            value={getStandardMetricDisplayUnit(widget.type, widget.data)}
            onValueChange={(value) => updateWidgetData(widget.id, { display_unit: value })}
            options={supportsUnitSelection ? unitOptions : undefined}
          />
        }
      />
    </>
  )
}
