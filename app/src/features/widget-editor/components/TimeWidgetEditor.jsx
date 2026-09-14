/**
 * Supports widget editing flows related to time widget editor.
 */

import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ELAPSED_TIME_ORIGINS, TIME_WIDGET_MODES } from '@/lib/widget/standard-widgets'
import { useTranslation } from 'react-i18next'
import { FieldBlock, ToggleField } from './widgetFormControls'
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
export default function TimeWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const isElapsed = widget.data.time_mode === 'elapsed'

  return (
    <>
      <div className="space-y-4">
        <FieldBlock label={t('widget-editor.timeMode', 'Time Mode')}>
          <ToggleGroup
            type="single"
            value={widget.data.time_mode}
            onValueChange={(timeMode) => {
              if (timeMode) updateWidgetData(widget.id, { time_mode: timeMode })
            }}
          >
            {TIME_WIDGET_MODES.map((mode) => (
              <ToggleGroupItem key={mode.value} value={mode.value} className="h-8 w-auto px-3 text-xs">
                {t(mode.labelKey, mode.defaultLabel)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </FieldBlock>

        {isElapsed ? (
          <>
            <FieldBlock label={t('widget-editor.elapsedFrom', 'Elapsed From')}>
              <ToggleGroup
                type="single"
                value={widget.data.elapsed_origin}
                onValueChange={(elapsedOrigin) => {
                  if (elapsedOrigin) updateWidgetData(widget.id, { elapsed_origin: elapsedOrigin })
                }}
              >
                {ELAPSED_TIME_ORIGINS.map((origin) => (
                  <ToggleGroupItem key={origin.value} value={origin.value} className="h-8 w-auto px-3 text-xs">
                    {t(origin.labelKey, origin.defaultLabel)}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </FieldBlock>
            <FieldBlock label={t('widget-editor.hundredths', 'Hundredths')}>
              <ToggleField
                checked={widget.data.show_hundredths}
                onCheckedChange={(showHundredths) => updateWidgetData(widget.id, { show_hundredths: showHundredths })}
              />
            </FieldBlock>
          </>
        ) : null}
      </div>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        showFormatSelect={!isElapsed}
        showContentAlignment
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
