/**
 * Renders the sidebar settings tab portion of the application interface.
 */

import { useEffect, useMemo, useState } from 'react'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import {
  Video,
  Palette,
  RotateCcw,
  Gauge,
  AlertTriangle,
  ChevronUp,
  ChevronDown,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import ExportRangeSettings from '@/components/ExportRangeSettings'
import FontSelectField from '@/components/ui/font-select-field'
import HexColorPicker from '@/components/ui/hex-color-picker'
import useAvailableFonts from '@/hooks/useAvailableFonts'
import {
  getContainerFps,
  getUpdateRateOptions,
  normalizeUpdateRateForFps,
  sanitizeIntegerFps,
} from '@/lib/update-rate'
import useStore from '../store/useStore'

const ASPECT_RATIOS = [
  { id: '16:9', name: 'Widescreen (16:9)' },
  { id: '9:16', name: 'Vertical (9:16)' },
  { id: '1:1', name: 'Square (1:1)' },
  { id: '4:3', name: 'Portrait (4:3)' },
  { id: '21:9', name: 'Ultrawide (21:9)' },
  { id: 'custom', name: 'Custom' },
]

const RESOLUTIONS = {
  '16:9': [
    { id: '4k', name: '4K (3840x2160)', w: 3840, h: 2160 },
    { id: '1080p', name: '1080p (1920x1080)', w: 1920, h: 1080 },
    { id: '720p', name: '720p (1280x720)', w: 1280, h: 720 },
  ],
  '9:16': [
    { id: '4k-v', name: '4K Vertical (2160x3840)', w: 2160, h: 3840 },
    { id: '1080p-v', name: '1080p Vertical (1080x1920)', w: 1080, h: 1920 },
  ],
  '1:1': [
    { id: '1080s', name: '1080p Square (1080x1080)', w: 1080, h: 1080 },
    { id: '2160s', name: '4K Square (2160x2160)', w: 2160, h: 2160 },
  ],
  '4:3': [
    { id: 'sxga', name: 'SXGA+ (1400x1050)', w: 1400, h: 1050 },
    { id: 'uxga', name: 'UXGA (1600x1200)', w: 1600, h: 1200 },
    { id: 'hires', name: 'XGA (1920x1440)', w: 1920, h: 1440 },
    { id: 'qxga', name: 'QXGA (2048x1536)', w: 2048, h: 1536 },
  ],
  '21:9': [{ id: 'ultra', name: 'Ultrawide (3440x1440)', w: 3440, h: 1440 }],
}

/**
 * Handles sanitize number.
 *
 * @param {*} val - Value for val.
 * @returns {*} Result produced by the helper.
 */
function sanitizeNumber(val) {
  if (val === undefined || val === null) return val
  const sanitized = val
    .toString()
    .replace(/,/g, '')
    .replace(/^0+(?!$)/, '')
  return parseInt(sanitized, 10) || 0
}

function parseTimeOffset(value) {
  if (!value) return 0
  const str = String(value).trim()
  if (str === '') return 0

  const isNegative = str.startsWith('-')
  const absStr = isNegative ? str.substring(1) : str

  if (absStr.includes(':')) {
    const parts = absStr.split(':')
    let seconds = 0
    if (parts.length === 2) {
      seconds = parseInt(parts[0]) * 60 + parseFloat(parts[1])
    } else if (parts.length === 3) {
      seconds =
        parseInt(parts[0]) * 3600 +
        parseInt(parts[1]) * 60 +
        parseFloat(parts[2])
    }
    return isNegative ? -seconds : seconds
  }

  const parsed = parseFloat(str)
  return isNaN(parsed) ? 0 : parsed
}

/**
 * Renders the sidebar settings tab component.
 *
 * @param {object} props - Component props.
 * @param {*} props.config - Overlay template configuration data.
 * @param {*} props.onConfigChange - Callback invoked to config change.
 * @returns {JSX.Element} Rendered component output.
 */
export default function SidebarSettingsTab({ config, onConfigChange }) {
  const {
    globalDefaults,
    setGlobalDefault,
    updateRate,
    setUpdateRate,
    exportRange,
    setExportRange,
    activitySummary,
    aspectRatio,
    setAspectRatio,
    resetGlobalDefaults,
    importedVideoPath,
    importedVideoFps,
    importedVideoDuration,
    importedVideoResolution,
    importedVideoCreationTime,
    videoSyncOffsetSeconds,
    videoSyncWarning,
    setVideoSyncOffset,
    computeVideoSync,
  } = useStore()

  const scene = config?.scene
  const systemFonts = useAvailableFonts()

  const [resId, setResId] = useState(() => {
    if (!scene) return '1080p'
    const match = Object.values(RESOLUTIONS)
      .flat()
      .find((r) => r.w === scene.width && r.h === scene.height)
    return match ? match.id : 'custom'
  })
  const [fpsMode, setFpsMode] = useState(
    [24, 30, 60].includes(scene?.fps) ? scene?.fps?.toString() : 'custom',
  )
  const updateRateOptions = useMemo(
    () => getUpdateRateOptions(scene?.fps),
    [scene?.fps],
  )
  const containerFps = useMemo(
    () => getContainerFps(scene?.fps, updateRate),
    [scene?.fps, updateRate],
  )

  const [offsetInput, setOffsetInput] = useState(
    videoSyncOffsetSeconds?.toString() || '0',
  )

  useEffect(() => {
    setOffsetInput(videoSyncOffsetSeconds?.toString() || '0')
  }, [videoSyncOffsetSeconds])

  const handleOffsetBlur = (val) => {
    const parsed = parseTimeOffset(val)
    const rounded = Math.round(parsed * 10) / 10
    setVideoSyncOffset(rounded)
    setOffsetInput(
      Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1),
    )
  }

  const handleIncrement = (amount) => {
    const current = parseTimeOffset(offsetInput)
    const newOffset = Math.round((current + amount) * 10) / 10
    setVideoSyncOffset(newOffset)
    setOffsetInput(
      Number.isInteger(newOffset) ? newOffset.toString() : newOffset.toFixed(1),
    )
  }

  useEffect(() => {
    if (scene) {
      const match = Object.values(RESOLUTIONS)
        .flat()
        .find((r) => r.w === scene.width && r.h === scene.height)
      setResId(match ? match.id : 'custom')
      if ([24, 30, 60].includes(scene.fps)) setFpsMode(scene.fps.toString())
      else setFpsMode('custom')
    }
  }, [scene])

  useEffect(() => {
    const normalizedUpdateRate = normalizeUpdateRateForFps(
      scene?.fps,
      updateRate,
    )
    if (normalizedUpdateRate !== updateRate) {
      setUpdateRate(normalizedUpdateRate)
    }
  }, [scene?.fps, setUpdateRate, updateRate])

  const updateScene = (key, value) => {
    let finalValue = value
    if (['width', 'height', 'x', 'y', 'start', 'end'].includes(key)) {
      finalValue = sanitizeNumber(value)
    }
    onConfigChange({ ...config, scene: { ...config.scene, [key]: finalValue } })
  }
  const sceneStyleValue = (key, fallback) =>
    globalDefaults?.[key] ?? scene?.[key] ?? fallback

  return (
    <div className="mt-4 space-y-8 outline-none pb-10">
      <div className="space-y-4">
        <div className="flex items-center gap-2 mb-2">
          <Video className="h-4 w-4 text-primary" />
          <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
            Overlay
          </h4>
          <Separator className="flex-1" />
        </div>

        <div className="grid grid-cols-2 gap-4 pt-4">
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">
              Aspect Ratio
            </Label>
            <Select
              value={aspectRatio}
              onValueChange={(v) => {
                setAspectRatio(v)
                if (v !== 'custom' && RESOLUTIONS[v]) {
                  const preset = RESOLUTIONS[v][0]
                  onConfigChange({
                    ...config,
                    scene: {
                      ...config.scene,
                      width: preset.w,
                      height: preset.h,
                    },
                  })
                }
              }}
            >
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
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">
              Resolution
            </Label>
            <Select
              value={resId}
              disabled={aspectRatio === 'custom'}
              onValueChange={(v) => {
                setResId(v)
                const preset = RESOLUTIONS[aspectRatio]?.find((r) => r.id === v)
                if (preset) {
                  onConfigChange({
                    ...config,
                    scene: {
                      ...config.scene,
                      width: preset.w,
                      height: preset.h,
                    },
                  })
                }
              }}
            >
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
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                Width
              </Label>
              <BlurInput
                type="number"
                value={scene?.width ?? ''}
                onChange={(e) => updateScene('width', e.target.value)}
                className="h-9 text-xs"
              />
            </div>
            <div className="space-y-2">
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                Height
              </Label>
              <BlurInput
                type="number"
                value={scene?.height ?? ''}
                onChange={(e) => updateScene('height', e.target.value)}
                className="h-9 text-xs"
              />
            </div>
          </div>
        )}

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">
              Framerate
            </Label>
            {importedVideoFps ? (
              <div className="flex h-9 items-center rounded-md border border-border/70 bg-surface-elevated px-3 text-xs text-muted-foreground">
                Locked to video FPS ({Math.round(importedVideoFps)} fps)
              </div>
            ) : (
              <Select
                value={fpsMode}
                onValueChange={(v) => {
                  setFpsMode(v)
                  if (v !== 'custom') {
                    const fps = sanitizeIntegerFps(v)
                    setUpdateRate(normalizeUpdateRateForFps(fps, updateRate))
                    updateScene('fps', fps)
                  }
                }}
              >
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
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                Custom FPS
              </Label>
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
                onChange={(e) => {
                  const fps = sanitizeIntegerFps(e.target.value)
                  setUpdateRate(normalizeUpdateRateForFps(fps, updateRate))
                  updateScene('fps', fps)
                }}
                className="h-9 text-xs"
              />
            </div>
          )}
        </div>
        <div className="space-y-3 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Gauge className="h-4 w-4 text-primary" />
              <Label className="text-xs font-semibold">
                Widget Update Rate
              </Label>
            </div>
          </div>
          <Tabs
            value={updateRate.toString()}
            onValueChange={(v) => setUpdateRate(parseInt(v))}
          >
            <TabsList
              className="grid h-8 w-full bg-surface p-0.5"
              style={{
                gridTemplateColumns: `repeat(${updateRateOptions.length}, minmax(0, 1fr))`,
              }}
            >
              {updateRateOptions.map((rate) => (
                <TabsTrigger
                  key={rate}
                  value={rate.toString()}
                  className="text-[10px] cursor-pointer"
                >
                  1/{rate}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <p className="text-[10px] text-muted-foreground">
            Output container: {containerFps.toFixed(2).replace(/\.00$/, '')} fps
          </p>
        </div>
        {activitySummary && !importedVideoPath ? (
          <div className="space-y-3 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
            <ExportRangeSettings
              exportRange={exportRange}
              onExportRangeChange={setExportRange}
            />
          </div>
        ) : null}

        {importedVideoPath ? (
          <div className="space-y-4 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
            <div className="flex items-center gap-2">
              <Video className="h-4 w-4 text-primary" />
              <Label className="text-xs font-semibold">Video Sync</Label>
            </div>

            <div className="space-y-2 text-[10px] text-muted-foreground">
              <div className="flex justify-between">
                <span>Info:</span>
                <span className="font-medium text-foreground">
                  {importedVideoDuration
                    ? `${Math.floor(importedVideoDuration / 60)}:${Math.floor(
                        importedVideoDuration % 60,
                      )
                        .toString()
                        .padStart(2, '0')} min`
                    : 'Unknown'}{' '}
                  ·{' '}
                  {importedVideoFps
                    ? `${Math.round(importedVideoFps * 100) / 100} fps`
                    : 'Unknown'}{' '}
                  ·{' '}
                  {importedVideoResolution
                    ? `${importedVideoResolution.width}×${importedVideoResolution.height}`
                    : 'Unknown'}
                </span>
              </div>
              <div className="flex justify-between">
                <span>Creation Time:</span>
                <span className="font-medium text-foreground">
                  {importedVideoCreationTime
                    ? new Date(importedVideoCreationTime).toLocaleString()
                    : 'Unknown'}
                </span>
              </div>
            </div>

            {videoSyncWarning && (
              <div className="flex gap-2 items-start rounded-md bg-destructive/10 p-2 text-destructive">
                <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />
                <p className="text-[10px] leading-tight">{videoSyncWarning}</p>
              </div>
            )}

            <div className="space-y-1">
              <Label className="text-[9px] text-muted-foreground uppercase font-bold">
                Sync Offset
              </Label>
              <div className="flex gap-2">
                <div className="relative flex-1">
                  <BlurInput
                    type="text"
                    value={offsetInput}
                    onChange={(e) => setOffsetInput(e.target.value)}
                    onBlur={(e) => handleOffsetBlur(e.target.value)}
                    className="h-9 bg-surface text-xs pr-11 w-full border border-border/70"
                    placeholder="Seconds or MM:SS"
                  />
                  <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
                    <button
                      type="button"
                      className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => handleIncrement(0.1)}
                    >
                      <ChevronUp className="h-3 w-3" />
                    </button>
                    <div className="h-px bg-border/60" />
                    <button
                      type="button"
                      className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => handleIncrement(-0.1)}
                    >
                      <ChevronDown className="h-3 w-3" />
                    </button>
                  </div>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-9 px-3 text-xs"
                  disabled={!activitySummary}
                  onClick={() => computeVideoSync(activitySummary)}
                >
                  Auto-sync
                </Button>
              </div>
            </div>
          </div>
        ) : null}
      </div>

      <div className="space-y-4">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <Palette className="h-4 w-4 text-primary" />
            <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
              Global Settings
            </h4>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            onClick={() => resetGlobalDefaults()}
          >
            <RotateCcw className="h-3 w-3" />
          </Button>
        </div>
        <Separator className="flex-1 mb-4" />

        <div className="grid grid-cols-2 gap-4">
          <FontSelectField
            label="Font - Values"
            value={globalDefaults.font_values}
            onValueChange={(v) => setGlobalDefault('font_values', v)}
            systemFonts={systemFonts}
          />
          <FontSelectField
            label="Font - Labels"
            value={globalDefaults.font_text}
            onValueChange={(v) => setGlobalDefault('font_text', v)}
            systemFonts={systemFonts}
          />
        </div>

        <div className="grid grid-cols-3 gap-3">
          <div className="space-y-2">
            <Label className="text-xs">Values</Label>
            <HexColorPicker
              value={globalDefaults.color_values}
              onChange={(value) => setGlobalDefault('color_values', value)}
              valueClassName="text-[10px] tracking-[0.16em]"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Labels</Label>
            <HexColorPicker
              value={globalDefaults.color_text}
              onChange={(value) => setGlobalDefault('color_text', value)}
              valueClassName="text-[10px] tracking-[0.16em]"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Icons</Label>
            <HexColorPicker
              value={globalDefaults.color_icons}
              onChange={(value) => setGlobalDefault('color_icons', value)}
              valueClassName="text-[10px] tracking-[0.16em]"
            />
          </div>
        </div>

        <div className="grid grid-cols-3 gap-4 pt-2">
          <div className="space-y-2">
            <Label className="text-xs">Borders</Label>
            <HexColorPicker
              value={sceneStyleValue('border_color', '#000000')}
              onChange={(value) => setGlobalDefault('border_color', value)}
              valueClassName="text-[10px] tracking-[0.16em]"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Shadows</Label>
            <HexColorPicker
              value={sceneStyleValue('shadow_color', '#000000')}
              onChange={(value) => setGlobalDefault('shadow_color', value)}
              valueClassName="text-[10px] tracking-[0.16em]"
            />
          </div>
        </div>

        <div className="space-y-6 pt-2">
          <div className="space-y-3 pt-2">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Transparency</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {Math.round(globalDefaults.opacity * 100)}%
              </span>
            </div>
            <Slider
              min={0}
              max={1}
              step={0.01}
              value={[globalDefaults.opacity]}
              onValueChange={([v]) => setGlobalDefault('opacity', v)}
            />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Scale</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {globalDefaults.scale.toFixed(2)}x
              </span>
            </div>
            <Slider
              min={0.5}
              max={2}
              step={0.01}
              value={[globalDefaults.scale]}
              onValueChange={([v]) => setGlobalDefault('scale', v)}
            />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Border Thickness</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {sceneStyleValue('border_thickness', 0)}px
              </span>
            </div>
            <Slider
              min={0}
              max={20}
              step={1}
              value={[sceneStyleValue('border_thickness', 0)]}
              onValueChange={([v]) => setGlobalDefault('border_thickness', v)}
            />
          </div>
          <div className="grid grid-cols-2 gap-6">
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <Label className="text-xs">Shadow Strength</Label>
                <span className="text-[10px] text-muted-foreground">
                  {sceneStyleValue('shadow_strength', 0)}
                </span>
              </div>
              <Slider
                min={0}
                max={20}
                step={1}
                value={[sceneStyleValue('shadow_strength', 0)]}
                onValueChange={([v]) => setGlobalDefault('shadow_strength', v)}
              />
            </div>
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <Label className="text-xs">Shadow Distance</Label>
                <span className="text-[10px] text-muted-foreground">
                  {sceneStyleValue('shadow_distance', 0)}
                </span>
              </div>
              <Slider
                min={0}
                max={20}
                step={1}
                value={[sceneStyleValue('shadow_distance', 0)]}
                onValueChange={([v]) => setGlobalDefault('shadow_distance', v)}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
