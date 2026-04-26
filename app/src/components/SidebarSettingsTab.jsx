import { useState, useEffect } from 'react'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Video, Palette, RotateCcw, Sparkles, Gauge } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import useStore from '../store/useStore'

const FONTS = [
  { id: 'Arial.ttf', name: 'Arial' },
  { id: 'Evogria.otf', name: 'Evogria' },
  { id: 'Furore.otf', name: 'Furore' },
]

const ASPECT_RATIOS = [
  { id: '16:9', name: 'Widescreen (16:9)' },
  { id: '9:16', name: 'Vertical (9:16)' },
  { id: '1:1', name: 'Square (1:1)' },
  { id: '4:5', name: 'Portrait (4:5)' },
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
  '4:5': [{ id: 'insta', name: 'Instagram (1080x1350)', w: 1080, h: 1350 }],
  '21:9': [{ id: 'ultra', name: 'Ultrawide (3440x1440)', w: 3440, h: 1440 }],
}

function sanitizeNumber(val) {
  if (val === undefined || val === null) return val
  const sanitized = val
    .toString()
    .replace(/,/g, '')
    .replace(/^0+(?!$)/, '')
  return parseInt(sanitized, 10) || 0
}

export default function SidebarSettingsTab({ config, onConfigChange }) {
  const {
    globalDefaults,
    setGlobalDefault,
    updateRate,
    setUpdateRate,
    exportRange,
    setExportRange,
    exportCodec,
    platformOs,
    setExportCodec,
    aspectRatio,
    setAspectRatio,
    resetGlobalDefaults,
  } = useStore()

  const scene = config?.scene
  const isVideoToolboxAvailable = platformOs === 'macos'

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

  const updateScene = (key, value) => {
    let finalValue = value
    if (['width', 'height', 'x', 'y', 'start', 'end'].includes(key)) {
      finalValue = sanitizeNumber(value)
    }
    onConfigChange({ ...config, scene: { ...config.scene, [key]: finalValue } })
  }

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

        <div className="grid grid-cols-2 gap-4">
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
              Resolution Preset
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
              Framerate (FPS)
            </Label>
            <Select
              value={fpsMode}
              onValueChange={(v) => {
                setFpsMode(v)
                if (v !== 'custom') updateScene('fps', parseInt(v))
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
          </div>
          {fpsMode === 'custom' && (
            <div className="space-y-2 animate-in fade-in slide-in-from-left-1">
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                Custom FPS
              </Label>
              <BlurInput
                type="number"
                min={1}
                value={scene?.fps ?? 30}
                onChange={(e) =>
                  updateScene('fps', parseInt(e.target.value) || 1)
                }
                className="h-9 text-xs"
              />
            </div>
          )}
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <Palette className="h-4 w-4 text-primary" />
            <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
              Global Styles
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
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">
              Value Font
            </Label>
            <Select
              value={globalDefaults.font_values}
              onValueChange={(v) => setGlobalDefault('font_values', v)}
            >
              <SelectTrigger className="h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FONTS.map((f) => (
                  <SelectItem key={f.id} value={f.id}>
                    {f.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">
              Label Font
            </Label>
            <Select
              value={globalDefaults.font_text}
              onValueChange={(v) => setGlobalDefault('font_text', v)}
            >
              <SelectTrigger className="h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FONTS.map((f) => (
                  <SelectItem key={f.id} value={f.id}>
                    {f.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="grid grid-cols-3 gap-3">
          <div className="space-y-2">
            <Label className="text-xs">Values</Label>
            <div className="flex gap-1 items-center">
              <Input
                type="color"
                value={globalDefaults.color_values}
                onChange={(e) =>
                  setGlobalDefault('color_values', e.target.value)
                }
                className="h-8 w-8 shrink-0 cursor-pointer overflow-hidden rounded border border-border/70 p-0"
              />
              <span className="text-[10px] font-mono opacity-50 uppercase">
                {globalDefaults.color_values.slice(1, 4)}
              </span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Labels</Label>
            <div className="flex gap-1 items-center">
              <Input
                type="color"
                value={globalDefaults.color_text}
                onChange={(e) => setGlobalDefault('color_text', e.target.value)}
                className="h-8 w-8 shrink-0 cursor-pointer overflow-hidden rounded border border-border/70 p-0"
              />
              <span className="text-[10px] font-mono opacity-50 uppercase">
                {globalDefaults.color_text.slice(1, 4)}
              </span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Icons</Label>
            <div className="flex gap-1 items-center">
              <Input
                type="color"
                value={globalDefaults.color_icons}
                onChange={(e) =>
                  setGlobalDefault('color_icons', e.target.value)
                }
                className="h-8 w-8 shrink-0 cursor-pointer overflow-hidden rounded border border-border/70 p-0"
              />
              <span className="text-[10px] font-mono opacity-50 uppercase">
                {globalDefaults.color_icons.slice(1, 4)}
              </span>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-4 pt-2">
          <div className="space-y-2">
            <Label className="text-xs">Border Color</Label>
            <div className="flex gap-2 items-center">
              <Input
                type="color"
                value={globalDefaults.border_color}
                onChange={(e) =>
                  setGlobalDefault('border_color', e.target.value)
                }
                className="h-8 w-8 shrink-0 cursor-pointer overflow-hidden rounded border border-border/70 p-0"
              />
              <span className="text-[10px] font-mono opacity-50 uppercase">
                {globalDefaults.border_color}
              </span>
            </div>
          </div>
          <div className="space-y-2">
            <Label className="text-xs">Shadow Color</Label>
            <div className="flex gap-2 items-center">
              <Input
                type="color"
                value={globalDefaults.shadow_color}
                onChange={(e) =>
                  setGlobalDefault('shadow_color', e.target.value)
                }
                className="h-8 w-8 shrink-0 cursor-pointer overflow-hidden rounded border border-border/70 p-0"
              />
              <span className="text-[10px] font-mono opacity-50 uppercase">
                {globalDefaults.shadow_color.slice(0, 7)}
              </span>
            </div>
          </div>
        </div>

        <div className="space-y-6 pt-2">
          <div className="space-y-3 pt-2">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Global Opacity</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {Math.round(globalDefaults.opacity * 100)}%
              </span>
            </div>
            <Slider
              min={0}
              max={1}
              step={0.05}
              value={[globalDefaults.opacity]}
              onValueChange={([v]) => setGlobalDefault('opacity', v)}
            />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Global Scale</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {globalDefaults.scale.toFixed(2)}x
              </span>
            </div>
            <Slider
              min={0.5}
              max={2}
              step={0.05}
              value={[globalDefaults.scale]}
              onValueChange={([v]) => setGlobalDefault('scale', v)}
            />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-xs">Border Thickness</Label>
              <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {globalDefaults.border_thickness}px
              </span>
            </div>
            <Slider
              min={0}
              max={20}
              step={1}
              value={[globalDefaults.border_thickness]}
              onValueChange={([v]) => setGlobalDefault('border_thickness', v)}
            />
          </div>
          <div className="grid grid-cols-2 gap-6">
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <Label className="text-xs">Shadow Strength</Label>
                <span className="text-[10px] text-muted-foreground">
                  {globalDefaults.shadow_strength}
                </span>
              </div>
              <Slider
                min={0}
                max={100}
                step={1}
                value={[globalDefaults.shadow_strength]}
                onValueChange={([v]) => setGlobalDefault('shadow_strength', v)}
              />
            </div>
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <Label className="text-xs">Shadow Distance</Label>
                <span className="text-[10px] text-muted-foreground">
                  {globalDefaults.shadow_distance}
                </span>
              </div>
              <Slider
                min={0}
                max={50}
                step={1}
                value={[globalDefaults.shadow_distance]}
                onValueChange={([v]) => setGlobalDefault('shadow_distance', v)}
              />
            </div>
          </div>
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex items-center gap-2 mb-2">
          <Sparkles className="h-4 w-4 text-primary" />
          <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
            Export
          </h4>
          <Separator className="flex-1" />
        </div>

        <div className="space-y-4">
          <div className="space-y-3 rounded-lg border border-accent-border bg-surface-accent-soft p-4">
            <div className="space-y-2">
              <Label className="text-xs font-semibold">Export Codec</Label>
              <Select value={exportCodec} onValueChange={setExportCodec}>
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="prores_ks">ProRes (CPU)</SelectItem>
                  <SelectItem value="prores_ks_vulkan">
                    ProRes Vulkan (GPU)
                  </SelectItem>
                  <SelectItem
                    value="prores_videotoolbox"
                    disabled={!isVideoToolboxAvailable}
                  >
                    ProRes (macOS)
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Gauge className="h-4 w-4 text-primary" />
                <Label className="text-xs font-semibold">
                  Widget Update Rate
                </Label>
              </div>
              <span className="rounded bg-surface-accent-strong px-1.5 py-0.5 text-[10px] font-mono text-highlight">
                Update 1/{updateRate}
              </span>
            </div>
            <Tabs
              value={updateRate.toString()}
              onValueChange={(v) => setUpdateRate(parseInt(v))}
            >
              <TabsList className="grid h-8 w-full grid-cols-4 bg-surface p-0.5">
                <TabsTrigger value="1" className="text-[10px] cursor-pointer">
                  1/1
                </TabsTrigger>
                <TabsTrigger value="2" className="text-[10px] cursor-pointer">
                  1/2
                </TabsTrigger>
                <TabsTrigger value="4" className="text-[10px] cursor-pointer">
                  1/4
                </TabsTrigger>
                <TabsTrigger value="8" className="text-[10px] cursor-pointer">
                  1/8
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>

          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs font-medium">
                  Custom Export Range
                </Label>
                <p className="text-[10px] text-muted-foreground">
                  Export specific section only
                </p>
              </div>
              <Switch
                checked={exportRange.type === 'custom'}
                onCheckedChange={(checked) =>
                  setExportRange({ type: checked ? 'custom' : 'all' })
                }
              />
            </div>
            {exportRange.type === 'custom' && (
              <div className="space-y-4 pt-2 animate-in fade-in slide-in-from-right-1 duration-200">
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-1.5">
                    <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                      From (Time or Sec)
                    </Label>
                    <BlurInput
                      value={exportRange.fromTime}
                      onChange={(e) =>
                        setExportRange({ fromTime: e.target.value })
                      }
                      className="h-8 text-xs font-mono"
                      placeholder="00:00:00 or 800"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-[10px] text-muted-foreground uppercase font-bold">
                      To (Time or Sec)
                    </Label>
                    <BlurInput
                      value={exportRange.toTime}
                      onChange={(e) =>
                        setExportRange({ toTime: e.target.value })
                      }
                      className="h-8 text-xs font-mono"
                      placeholder="00:00:00 or 900"
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
