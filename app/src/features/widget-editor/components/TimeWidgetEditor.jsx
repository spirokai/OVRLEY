/**
 * Supports widget editing flows related to time widget editor.
 */

import { FontSection, IconSection } from './widgetEditorSections'
import { ContentAlignmentControl } from './widgetFormControls'

/**
 * Renders the time widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TimeWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        showFormatSelect
      />
      <ContentAlignmentControl
        value={widget.data.content_alignment}
        onValueChange={(value) => updateWidgetData(widget.id, { content_alignment: value })}
      />
      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        setNumericField={setNumericField}
      />
    </>
  )
}
