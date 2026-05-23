/**
 * Renders global default settings — font selection, color pickers, opacity,
 * scale, border thickness, and shadow controls.
 * Pure presentational — all data comes from props.
 *
 * @param {object} props
 * @param {object} props.globalDefaults - Global default values.
 * @param {function} props.onGlobalDefaultChange - Callback to set a global default.
 * @param {function} props.onResetDefaults - Callback to reset all global defaults.
 * @param {function} props.sceneStyleValue - Helper to resolve scene vs global default.
 * @param {string[]} props.systemFonts - List of available system fonts.
 * @returns {JSX.Element} Rendered global settings section.
 */

import { Label } from '@/components/ui/label'
import { Slider } from '@/components/ui/slider'
import { Separator } from '@/components/ui/separator'
import { Palette, RotateCcw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import FontSelectField from '@/components/ui/font-select-field'
import HexColorPicker from '@/components/ui/hex-color-picker'

export default function GlobalSettingsSection({ globalDefaults, onGlobalDefaultChange, onResetDefaults, sceneStyleValue, systemFonts }) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Palette className="h-4 w-4 text-primary" />
          <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">Global Settings</h4>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
          onClick={onResetDefaults}
        >
          <RotateCcw className="h-3 w-3" />
        </Button>
      </div>
      <Separator className="flex-1 mb-4" />

      <div className="grid grid-cols-2 gap-4">
        <FontSelectField
          label="Font - Values"
          value={globalDefaults.font_values}
          onValueChange={(v) => onGlobalDefaultChange('font_values', v)}
          systemFonts={systemFonts}
        />
        <FontSelectField
          label="Font - Labels"
          value={globalDefaults.font_text}
          onValueChange={(v) => onGlobalDefaultChange('font_text', v)}
          systemFonts={systemFonts}
        />
      </div>

      <div className="grid grid-cols-3 gap-3">
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Values</Label>
          <HexColorPicker
            value={globalDefaults.color_values}
            onChange={(value) => onGlobalDefaultChange('color_values', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Labels</Label>
          <HexColorPicker
            value={globalDefaults.color_text}
            onChange={(value) => onGlobalDefaultChange('color_text', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Icons</Label>
          <HexColorPicker
            value={globalDefaults.color_icons}
            onChange={(value) => onGlobalDefaultChange('color_icons', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
      </div>

      <div className="grid grid-cols-3 gap-4 pt-2">
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Borders</Label>
          <HexColorPicker
            value={sceneStyleValue('border_color', '#000000')}
            onChange={(value) => onGlobalDefaultChange('border_color', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Shadows</Label>
          <HexColorPicker
            value={sceneStyleValue('shadow_color', '#000000')}
            onChange={(value) => onGlobalDefaultChange('shadow_color', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
        <div className="space-y-2">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">Units</Label>
          <HexColorPicker
            value={globalDefaults.color_units}
            onChange={(value) => onGlobalDefaultChange('color_units', value)}
            valueClassName="text-[10px] tracking-[0.16em]"
          />
        </div>
      </div>

      <div className="space-y-6 pt-2">
        <div className="space-y-3 pt-2">
          <div className="flex justify-between items-center">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Transparency</Label>
            <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {Math.round(globalDefaults.opacity * 100)}%
            </span>
          </div>
          <Slider min={0} max={1} step={0.01} value={[globalDefaults.opacity]} onValueChange={([v]) => onGlobalDefaultChange('opacity', v)} />
        </div>
        <div className="space-y-3">
          <div className="flex justify-between items-center">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Scale</Label>
            <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">{globalDefaults.scale.toFixed(2)}x</span>
          </div>
          <Slider min={0.5} max={2} step={0.01} value={[globalDefaults.scale]} onValueChange={([v]) => onGlobalDefaultChange('scale', v)} />
        </div>
        <div className="space-y-3">
          <div className="flex justify-between items-center">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">Border Thickness</Label>
            <span className="rounded bg-surface-strong px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {sceneStyleValue('border_thickness', 0)}px
            </span>
          </div>
          <Slider
            min={0}
            max={20}
            step={1}
            value={[sceneStyleValue('border_thickness', 0)]}
            onValueChange={([v]) => onGlobalDefaultChange('border_thickness', v)}
          />
        </div>
        <div className="grid grid-cols-2 gap-6">
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">Shadow Strength</Label>
              <span className="text-[10px] text-muted-foreground">{sceneStyleValue('shadow_strength', 0)}</span>
            </div>
            <Slider
              min={0}
              max={20}
              step={1}
              value={[sceneStyleValue('shadow_strength', 0)]}
              onValueChange={([v]) => onGlobalDefaultChange('shadow_strength', v)}
            />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <Label className="text-[10px] text-muted-foreground uppercase font-bold">Shadow Distance</Label>
              <span className="text-[10px] text-muted-foreground">{sceneStyleValue('shadow_distance', 0)}</span>
            </div>
            <Slider
              min={0}
              max={20}
              step={1}
              value={[sceneStyleValue('shadow_distance', 0)]}
              onValueChange={([v]) => onGlobalDefaultChange('shadow_distance', v)}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
