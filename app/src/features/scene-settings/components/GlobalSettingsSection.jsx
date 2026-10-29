/**
 * Renders global default settings — font selection, fixed-width digits, color
 * pickers, opacity, scale, border thickness, and shadow controls.
 * Pure presentational — all data comes from props.
 *
 * @param {object} props
 * @param {object} props.globalDefaults - Global default values.
 * @param {function} props.onGlobalDefaultChange - Callback to set a global default.
 * @param {function} props.onGlobalDefaultDraft - Callback to preview a global default during a slider drag.
 * @param {function} props.onGlobalDefaultCommit - Callback to commit a previewed global default.
 * @param {function} props.onResetDefaults - Callback to reset all global defaults.
 * @param {function} props.sceneStyleValue - Helper to resolve scene vs global default.
 * @param {object} props.availableFonts - Bundled recommended and system fonts.
 * @returns {JSX.Element} Rendered global settings section.
 */

import { Label } from '@/components/ui/label'
import { ColorField, CONTROL_CLASS, FIELD_LABEL_CLASS, SliderField, ToggleField } from '@/features/widget-editor/components/widgetFormControls'
import { Separator } from '@/components/ui/separator'
import { Palette, RotateCcw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import FontSelectField from '@/components/ui/font-select-field'
import { useTranslation } from 'react-i18next'

export default function GlobalSettingsSection({
  globalDefaults,
  onGlobalDefaultChange,
  onGlobalDefaultDraft,
  onGlobalDefaultCommit,
  onResetDefaults,
  sceneStyleValue,
  availableFonts,
}) {
  const { t } = useTranslation()
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-3 flex-1">
          <Palette className="h-4 w-4 text-primary" />
          <h4 className="text-[0.741rem] font-bold text-muted-foreground uppercase tracking-widest">
            {t('scene-settings.globalSettings', 'Global Settings')}
          </h4>
          <Separator className="flex-1" />
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="ml-2 h-6 w-6 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
          onClick={onResetDefaults}
        >
          <RotateCcw className="h-3 w-3" />
        </Button>
      </div>

      <div className="grid grid-cols-2 gap-4 pt-4">
        <FontSelectField
          label={t('scene-settings.fontValues', 'Font - Values')}
          value={globalDefaults.font_values}
          onValueChange={(v) => onGlobalDefaultChange('font_values', v)}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName={CONTROL_CLASS}
          labelClassName={FIELD_LABEL_CLASS}
        />
        <FontSelectField
          label={t('scene-settings.fontLabels', 'Font - Labels')}
          value={globalDefaults.font_text}
          onValueChange={(v) => onGlobalDefaultChange('font_text', v)}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName={CONTROL_CLASS}
          labelClassName={FIELD_LABEL_CLASS}
        />
      </div>

      <div className="flex items-center justify-between">
        <Label className={FIELD_LABEL_CLASS}>{t('scene-settings.fixedWidthDigits', 'Fixed-Width Digits')}</Label>
        <ToggleField
          label={t('scene-settings.fixedWidthDigits', 'Fixed-Width Digits')}
          checked={globalDefaults.fixed_width_digits}
          onCheckedChange={(checked) => onGlobalDefaultChange('fixed_width_digits', checked)}
        />
      </div>

      <div className="grid grid-cols-3 gap-3 pt-4">
        <ColorField
          label={t('scene-settings.values', 'Values')}
          value={globalDefaults.color_values}
          onChange={(value) => onGlobalDefaultChange('color_values', value)}
        />
        <ColorField
          label={t('scene-settings.labels', 'Labels')}
          value={globalDefaults.color_text}
          onChange={(value) => onGlobalDefaultChange('color_text', value)}
        />
        <ColorField
          label={t('scene-settings.icons', 'Icons')}
          value={globalDefaults.color_icons}
          onChange={(value) => onGlobalDefaultChange('color_icons', value)}
        />
      </div>

      <div className="grid grid-cols-3 gap-3">
        <ColorField
          label={t('scene-settings.borders', 'Borders')}
          value={sceneStyleValue('border_color', '#000000')}
          onChange={(value) => onGlobalDefaultChange('border_color', value)}
        />
        <ColorField
          label={t('scene-settings.shadows', 'Shadows')}
          value={sceneStyleValue('shadow_color', '#000000')}
          onChange={(value) => onGlobalDefaultChange('shadow_color', value)}
        />
        <ColorField
          label={t('scene-settings.units', 'Units')}
          value={globalDefaults.color_units}
          onChange={(value) => onGlobalDefaultChange('color_units', value)}
        />
      </div>

      <div className="space-y-6 pt-6">
        <SliderField
          editable
          label={t('scene-settings.transparency', 'Transparency')}
          min={0}
          max={1}
          step={0.01}
          value={globalDefaults.opacity}
          valueScale={100}
          valueDisplay={`${Math.round(globalDefaults.opacity * 100)}`}
          suffix="%"
          onSliderChange={(value) => onGlobalDefaultDraft('opacity', value)}
          onSliderCommit={() => onGlobalDefaultCommit('opacity')}
        />
        <SliderField
          editable
          label={t('scene-settings.scale', 'Scale')}
          min={0.5}
          max={2}
          step={0.01}
          value={globalDefaults.scale}
          valueDisplay={`${globalDefaults.scale.toFixed(2)}`}
          suffix="x"
          onSliderChange={(value) => onGlobalDefaultDraft('scale', value)}
          onSliderCommit={() => onGlobalDefaultCommit('scale')}
        />
        <SliderField
          editable
          label={t('scene-settings.borderThickness', 'Border Thickness')}
          min={0}
          max={20}
          value={sceneStyleValue('border_thickness', 0)}
          valueDisplay={`${sceneStyleValue('border_thickness', 0)}`}
          suffix="px"
          onSliderChange={(value) => onGlobalDefaultDraft('border_thickness', value)}
          onSliderCommit={() => onGlobalDefaultCommit('border_thickness')}
        />
        <div className="grid grid-cols-2 gap-6">
          <SliderField
            editable
            label={t('scene-settings.shadowStrength', 'Shadow Strength')}
            min={0}
            max={20}
            value={sceneStyleValue('shadow_strength', 0)}
            valueDisplay={String(sceneStyleValue('shadow_strength', 0))}
            onSliderChange={(value) => onGlobalDefaultDraft('shadow_strength', value)}
            onSliderCommit={() => onGlobalDefaultCommit('shadow_strength')}
          />
          <SliderField
            editable
            label={t('scene-settings.shadowDistance', 'Shadow Distance')}
            min={0}
            max={20}
            value={sceneStyleValue('shadow_distance', 0)}
            valueDisplay={String(sceneStyleValue('shadow_distance', 0))}
            onSliderChange={(value) => onGlobalDefaultDraft('shadow_distance', value)}
            onSliderCommit={() => onGlobalDefaultCommit('shadow_distance')}
          />
        </div>
      </div>
    </div>
  )
}
