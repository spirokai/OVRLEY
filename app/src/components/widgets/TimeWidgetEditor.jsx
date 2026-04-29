/**
 * Supports widget editing flows related to time widget editor.
 */

import { FontSection, IconSection } from './widgetEditorSections'

/**
 * Renders the time widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TimeWidgetEditor({
  widget,
  updateWidgetData,
  setNumericField,
}) {
  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        showFormatSelect
      />
      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    </>
  )
}
