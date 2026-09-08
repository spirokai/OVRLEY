/**
 * Supports widget editing flows related to text widget editor.
 */

import { FontSection } from './widgetEditorSections'
import { useTranslation } from 'react-i18next'

/**
 * Renders the text widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TextWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        title={t('widget-editor.typography', 'Typography')}
        showTextInput
        colorLabel={t('widget-editor.color', 'Color')}
      />
    </>
  )
}
