/**
 * Metric widget editor — dispatches to the appropriate display-type section.
 *
 * Text display uses TextDisplaySection. Non-text display types are resolved
 * via the DISPLAY_SECTION registry. Adding a new display type only requires
 * creating a new section component and registering it below.
 */

import { getDisplayTypeDefaultFontSize, getDisplayTypeOptions, getStandardMetricDisplayUnit } from '@/lib/widget/standard-metrics'
import { NumberField, SelectField } from '../widgetFormControls'
import { useCallback } from 'react'
import { initDisplayVariant } from '@/lib/widget/widget-resolver'
import { isTextDisplayType } from '@/lib/widget/display-type-behavior'
import TextDisplaySection from './TextDisplaySection'
import LinearDisplaySection from './LinearDisplaySection'
import ArcDisplaySection from './ArcDisplaySection'
import HeadingTapeDisplaySection from './HeadingTapeDisplaySection'
import LeanAngleDisplaySection from './LeanAngleDisplaySection'
import GForceWidgetEditor from '../GForceWidgetEditor'
import LapTimerDisplaySection from './LapTimerDisplaySection'

/**
 * Registry mapping display_type values to their editor section components.
 * Each section receives { widget, updateWidgetData } and manages its own
 * variant data extraction and update logic.
 */
const DISPLAY_SECTION = {
  heading_tape: HeadingTapeDisplaySection,
  linear: LinearDisplaySection,
  arc: ArcDisplaySection,
  corner: ArcDisplaySection,
  lean_angle: LeanAngleDisplaySection,
  g_force: GForceWidgetEditor,
  lap_timer: LapTimerDisplaySection,
}

/**
 * @param {object} props
 * @param {object} props.widget - Widget definition being rendered or edited.
 * @param {Function} props.updateWidgetData - Updates widget data immutably.
 * @param {Function} props.setNumericField - Sets a numeric field on the widget.
 * @param {boolean} props.showDisplayControls - Whether to show the display type header + dropdown.
 * @returns {JSX.Element}
 */
export default function MetricWidgetEditor({
  widget,
  updateWidgetData,
  updateWidgetSize,
  commitWidgetSize,
  setNumericField,
  showDisplayControls = true,
}) {
  const displayType = widget.data.display_type || 'text'
  const displayOptions = getDisplayTypeOptions(widget.type)

  const handleDisplayTypeChange = useCallback(
    (value) => {
      const nextData = initDisplayVariant(widget.data, value)
      const patch = { display_type: value, display_variants: nextData.display_variants }
      const defaultFontSize = getDisplayTypeDefaultFontSize(value)
      if (defaultFontSize !== null) patch.font_size = defaultFontSize
      updateWidgetData(widget.id, patch)
    },
    [widget.id, widget.data, updateWidgetData],
  )

  const showTypeDropdown = showDisplayControls && displayOptions.length > 1
  const DisplaySection = DISPLAY_SECTION[displayType]

  return (
    <>
      {showTypeDropdown ? (
        <div className="space-y-2">
          {/* <SectionHeading icon={Gauge} title="Display" /> */}
          <SelectField label="Display Type" value={displayType} onValueChange={handleDisplayTypeChange} options={displayOptions} />
        </div>
      ) : null}

      {isTextDisplayType(displayType) ? (
        <TextDisplaySection
          widget={widget}
          updateWidgetData={updateWidgetData}
          updateWidgetSize={updateWidgetSize}
          commitWidgetSize={commitWidgetSize}
          setNumericField={setNumericField}
        />
      ) : DisplaySection ? (
        <DisplaySection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />
      ) : null}

      {widget.type === 'altitude' ? (
        <div className="grid grid-cols-2 gap-4">
          <NumberField
            label="Altitude at start"
            value={widget.data.starting_altitude}
            suffix={getStandardMetricDisplayUnit(widget.type, widget.data)}
            onChange={(rawValue) => setNumericField(widget.id, 'starting_altitude', rawValue, { optional: true, round: true })}
            onReset={() => updateWidgetData(widget.id, { starting_altitude: null })}
          />
        </div>
      ) : null}
    </>
  )
}
