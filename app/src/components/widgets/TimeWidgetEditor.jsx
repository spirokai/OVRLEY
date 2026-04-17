import {
  FontSection,
  IconSection,
  OpacitySection,
} from './widgetEditorSections'

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
      <OpacitySection widget={widget} updateWidgetData={updateWidgetData} />
    </>
  )
}
