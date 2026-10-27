/**
 * Renders overlay video settings including aspect ratio, resolution, framerate,
 * widget update rate, and export range controls.
 * Pure presentational — all data comes from props.
 *
 * @param {object} props
 * @param {string} props.aspectRatio - Current aspect ratio ID.
 * @param {function} props.onAspectRatioChange - Callback for aspect ratio change.
 * @param {string} props.resId - Current resolution preset ID.
 * @param {function} props.onResChange - Callback for resolution preset change.
 * @param {object} props.scene - Current scene config (width, height, fps).
 * @param {function} props.onUpdateScene - Callback to update a scene key.
 * @param {number} props.importedVideoFps - Imported video FPS (or null).
 * @param {string} props.fpsMode - Current FPS mode (e.g. "24", "30", "custom").
 * @param {function} props.onFpsModeChange - Callback for FPS mode change.
 * @param {function} props.onCustomFpsChange - Callback for custom FPS input.
 * @param {number} props.updateRate - Current update rate divisor.
 * @param {number[]} props.updateRateOptions - Available update rate divisors.
 * @param {function} props.onUpdateRateChange - Callback for update rate change.
 * @param {boolean} props.hasTimelineMedia - Whether an activity or videos define the timeline.
 * @param {object} props.exportRange - Export range state.
 * @param {number} props.renderFps - Render frame rate the export range snaps to.
 * @param {function} props.onExportRangeChange - Callback for export range change.
 * @returns {JSX.Element} Rendered overlay settings section.
 */

import { Label } from '@/components/ui/label'
import { CONTROL_CLASS, FieldBlock, NumberField, SelectField } from '@/features/widget-editor/components/widgetFormControls'
import { Separator } from '@/components/ui/separator'
import { Video, Gauge, Bell } from 'lucide-react'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ExportRangeSettings } from '@/features/render-video'
import { ASPECT_RATIOS, RESOLUTIONS } from '../data/sceneSettingsConstants'
import { useTranslation } from 'react-i18next'

export default function OverlaySettingsSection({
  aspectRatio,
  onAspectRatioChange,
  resId,
  onResChange,
  scene,
  onUpdateScene,
  importedVideoFps,
  fpsMode,
  onFpsModeChange,
  onCustomFpsChange,
  updateRate,
  updateRateOptions,
  onUpdateRateChange,
  hasTimelineMedia,
  exportRange,
  renderFps,
  onExportRangeChange,
  videoResolutionMismatch,
}) {
  const { t } = useTranslation()
  return (
    <>
      <div className="flex items-center gap-3 mb-2">
        <Video className="h-4 w-4 text-primary" />
        <h4 className="text-[0.741rem] font-bold text-muted-foreground uppercase tracking-widest">{t('scene-settings.overlay', 'Overlay')}</h4>
        <Separator className="flex-1" />
      </div>

      <div className="grid grid-cols-2 gap-4 pt-4">
        <SelectField
          label={t('scene-settings.aspectRatio', 'Aspect Ratio')}
          value={aspectRatio}
          onValueChange={onAspectRatioChange}
          options={ASPECT_RATIOS.map((r) => ({ value: r.id, label: t(r.labelKey) }))}
        />
        <SelectField
          label={t('scene-settings.resolution', 'Resolution')}
          value={resId}
          disabled={aspectRatio === 'custom'}
          onValueChange={onResChange}
          options={[
            ...(RESOLUTIONS[aspectRatio] || []).map((r) => ({ value: r.id, label: t(r.labelKey) })),
            { value: 'custom', label: t('scene-settings.custom', 'Custom') },
          ]}
        />
      </div>

      {(aspectRatio === 'custom' || resId === 'custom') && (
        <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-1">
          <NumberField label={t('scene-settings.width', 'Width')} value={scene?.width ?? ''} onChange={(value) => onUpdateScene('width', value)} />
          <NumberField
            label={t('scene-settings.height', 'Height')}
            value={scene?.height ?? ''}
            onChange={(value) => onUpdateScene('height', value)}
          />
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        {importedVideoFps ? (
          <FieldBlock label={t('scene-settings.framerate', 'Framerate')}>
            <div className={`${CONTROL_CLASS} flex items-center rounded-sm border px-3 text-muted-foreground cursor-not-allowed opacity-50`}>
              {t('scene-settings.lockedToFps', 'Locked to {{fps}} fps', { fps: Math.round(importedVideoFps) })}
            </div>
          </FieldBlock>
        ) : (
          <SelectField
            label={t('scene-settings.framerate', 'Framerate')}
            value={fpsMode}
            onValueChange={onFpsModeChange}
            options={[
              { value: '24', label: '24 fps' },
              { value: '30', label: '30 fps' },
              { value: '60', label: '60 fps' },
              { value: 'custom', label: t('scene-settings.custom', 'Custom') },
            ]}
          />
        )}
        {!importedVideoFps && fpsMode === 'custom' && (
          <NumberField label={t('scene-settings.customFps', 'Custom FPS')} value={scene?.fps ?? 30} min={1} step={1} onChange={onCustomFpsChange} />
        )}
      </div>

      {videoResolutionMismatch && (
        <div className="flex gap-2 items-center rounded-sm bg-amber-500/15 p-2 pl-4 text-amber-400">
          <Bell className="h-3 w-3 shrink-0" />
          <p className="text-[0.65rem] font-semibold leading-tight">
            {t('scene-settings.resolutionMismatch', 'Overlay and video resolutions do not match')}
          </p>
        </div>
      )}

      <div className="space-y-3 rounded-xs border-none bg-surface-accent-soft/70 p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Gauge className="h-4 w-4 text-primary" />
            <Label className="text-xs font-semibold">{t('scene-settings.widgetUpdateRate', 'Widget Update Rate')}</Label>
          </div>
        </div>
        <Tabs value={updateRate.toString()} onValueChange={onUpdateRateChange}>
          <TabsList
            className="grid h-8 w-full bg-surface p-0.5"
            style={{
              gridTemplateColumns: `repeat(${updateRateOptions.length}, minmax(0, 1fr))`,
            }}
          >
            {updateRateOptions.map((rate) => (
              <TabsTrigger key={rate} value={rate.toString()} className="text-[0.75rem] cursor-pointer font-bold">
                1/{rate}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </div>

      {hasTimelineMedia ? (
        <div className="space-y-3 rounded-xs border-none bg-surface-accent-soft/70 p-4">
          <ExportRangeSettings range={exportRange} fps={renderFps} onExportRangeChange={onExportRangeChange} />
        </div>
      ) : null}
    </>
  )
}
