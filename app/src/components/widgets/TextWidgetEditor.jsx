import { FontSection, OpacitySection } from './widgetEditorSections'

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
