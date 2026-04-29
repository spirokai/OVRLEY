/**
 * Supports widget editing flows related to text widget editor.
 */

import { FontSection, OpacitySection } from './widgetEditorSections'

/**
 * Renders the text widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TextWidgetEditor({ widget, updateWidgetData }) {
  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        title="Text Content"
        showTextInput
        colorLabel="Color"
      />
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
