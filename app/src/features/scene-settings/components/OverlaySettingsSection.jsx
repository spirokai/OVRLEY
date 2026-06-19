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
 * @param {object} props.activitySummary - Activity summary data (or null).
 * @param {string} props.importedVideoPath - Imported video path (or null).
 * @param {object} props.exportRange - Export range state.
 * @param {function} props.onExportRangeChange - Callback for export range change.
 * @returns {JSX.Element} Rendered overlay settings section.
 */

import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Video, Gauge } from 'lucide-react'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ExportRangeSettings } from '@/features/render-video'
import { ASPECT_RATIOS, RESOLUTIONS } from '../data/sceneSettingsConstants'

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
  activitySummary,
  importedVideoPath,
  exportRange,
  onExportRangeChange,
}) {
  return (
    <>
      <div className="flex items-center gap-3 mb-2">
        <Video className="h-4 w-4 text-primary" />
        <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">Overlay</h4>
        <Separator className="flex-1" />
      </div>

      <div className="grid grid-cols-2 gap-4 pt-4">
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Aspect Ratio</Label>
          <Select value={aspectRatio} onValueChange={onAspectRatioChange}>
            <SelectTrigger className="h-9 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ASPECT_RATIOS.map((r) => (
                <SelectItem key={r.id} value={r.id}>
                  {r.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Resolution</Label>
          <Select value={resId} disabled={aspectRatio === 'custom'} onValueChange={onResChange}>
            <SelectTrigger className="h-9 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(RESOLUTIONS[aspectRatio] || []).map((r) => (
                <SelectItem key={r.id} value={r.id}>
                  {r.name}
                </SelectItem>
              ))}
              <SelectItem value="custom">Custom</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {(aspectRatio === 'custom' || resId === 'custom') && (
        <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-1">
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Width</Label>
            <BlurInput type="number" value={scene?.width ?? ''} onChange={(e) => onUpdateScene('width', e.target.value)} className="h-9 text-xs" />
          </div>
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Height</Label>
            <BlurInput type="number" value={scene?.height ?? ''} onChange={(e) => onUpdateScene('height', e.target.value)} className="h-9 text-xs" />
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Framerate</Label>
          {importedVideoFps ? (
            <div className="flex h-9 items-center rounded-md border border-border/70 bg-surface-elevated px-3 text-xs text-muted-foreground cursor-not-allowed opacity-50">
              Locked to {Math.round(importedVideoFps)} fps
            </div>
          ) : (
            <Select value={fpsMode} onValueChange={onFpsModeChange}>
              <SelectTrigger className="h-9 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="24">24 fps</SelectItem>
                <SelectItem value="30">30 fps</SelectItem>
                <SelectItem value="60">60 fps</SelectItem>
                <SelectItem value="custom">Custom</SelectItem>
              </SelectContent>
            </Select>
          )}
        </div>
        {!importedVideoFps && fpsMode === 'custom' && (
          <div className="space-y-2 animate-in fade-in slide-in-from-left-1">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Custom FPS</Label>
            <BlurInput
              type="number"
              min={1}
              step={1}
              inputMode="numeric"
              value={scene?.fps ?? 30}
              onKeyDown={(event) => {
                if (['.', ',', 'e', 'E', '+', '-'].includes(event.key)) {
                  event.preventDefault()
                }
              }}
              onChange={onCustomFpsChange}
              className="h-9 text-xs"
            />
          </div>
        )}
      </div>

      <div className="space-y-3 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Gauge className="h-4 w-4 text-primary" />
            <Label className="text-xs font-semibold">Widget Update Rate</Label>
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
              <TabsTrigger key={rate} value={rate.toString()} className="text-[10px] cursor-pointer">
                1/{rate}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </div>

      {activitySummary && !importedVideoPath ? (
        <div className="space-y-3 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
          <ExportRangeSettings exportRange={exportRange} onExportRangeChange={onExportRangeChange} />
        </div>
      ) : null}
    </>
  )
}
