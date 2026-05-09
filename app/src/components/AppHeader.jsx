/**
 * Renders the app header portion of the application interface.
 */

import { useMemo } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import {
  Activity,
  FilePlus2,
  FolderOpen,
  Grid3X3,
  LayoutGrid,
  Magnet,
  Minus,
  Play,
  RotateCcw,
  Save,
  Sparkles,
  Square,
  ZoomIn,
} from 'lucide-react'

function getTemplateResolution(template) {
  const width = Number(template.width ?? template.scene?.width)
  const height = Number(template.height ?? template.scene?.height)

  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    width <= 0 ||
    height <= 0
  ) {
    return null
  }

  return {
    width,
    height,
    label: `${width} x ${height}`,
    pixels: width * height,
  }
}

function getTemplateGroups(templates) {
  const groupsByKey = new Map()

  templates.forEach((template) => {
    const resolution = getTemplateResolution(template)
    const key = resolution ? resolution.label : 'Unknown Resolution'
    const group = groupsByKey.get(key) || {
      key,
      label: key,
      pixels: resolution?.pixels ?? -1,
      width: resolution?.width ?? -1,
      height: resolution?.height ?? -1,
      templates: [],
    }

    group.templates.push(template)
    groupsByKey.set(key, group)
  })

  return [...groupsByKey.values()]
    .map((group) => ({
      ...group,
      templates: group.templates.sort((left, right) =>
        left.name.localeCompare(right.name, undefined, {
          sensitivity: 'base',
        }),
      ),
    }))
    .sort((left, right) => {
      if (right.pixels !== left.pixels) return right.pixels - left.pixels
      if (right.width !== left.width) return right.width - left.width
      if (right.height !== left.height) return right.height - left.height
      return left.label.localeCompare(right.label, undefined, {
        sensitivity: 'base',
      })
    })
}

/**
 * Renders the app header component.
 *
 * @param {object} props - Component props.
 * @param {*} props.activityControls - Activity control state and handlers.
 * @param {*} props.backendStatus - Current backend status.
 * @param {*} props.editorControls - Editor control state and handlers.
 * @param {*} props.onOpenDownloads - Callback invoked to open downloads.
 * @param {*} props.renderControls - Render control state and handlers.
 * @param {*} props.templateControls - Template control state and handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function AppHeader({
  activityControls,
  backendStatus,
  editorControls,
  onOpenDownloads,
  renderControls,
  templateControls,
}) {
  const { activityLabel, onOpenActivityFile } = activityControls
  const {
    backgroundMode,
    gridVisible,
    onResetZoom,
    onSetBackgroundMode,
    onSetGridVisible,
    onSetSnapToGrid,
    onZoomIn,
    onZoomOut,
    snapToGrid,
    zoomLevel,
  } = editorControls
  const {
    onOpenRenderDialog,
    renderDisabled,
    renderTooltipContent,
    renderingVideo,
  } = renderControls
  const {
    config,
    handleCreateNewTemplate,
    handleImportTemplate,
    handleSaveTemplate,
    handleTemplateChange,
    loadedTemplateFilename,
    loadedTemplateSource,
    showTemplateStatus,
    status,
    templates,
  } = templateControls
  const templateGroups = useMemo(
    () => getTemplateGroups(templates),
    [templates],
  )

  return (
    <header className="relative z-50 shrink-0 border-b border-border/70 bg-card/80 backdrop-blur-sm">
      <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-4 px-6 py-3">
        <div className="flex min-w-0 items-center gap-6">
          <div className="flex items-center gap-3">
            <img src="/logo.svg" alt="OVRLEY" className="h-5" />
          </div>

          <div className="h-8 w-px bg-border/60" />

          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <Button
                className="mr-4 h-9 gap-2 border-border/70 px-5"
                onClick={onOpenActivityFile}
              >
                <Activity className="h-3.5 w-3.5" />
                <span className="max-w-28 truncate">{activityLabel}</span>
              </Button>

              <Select
                value={
                  loadedTemplateSource === 'backend'
                    ? loadedTemplateFilename || ''
                    : ''
                }
                onValueChange={handleTemplateChange}
              >
                <SelectTrigger className="h-8 w-56 bg-surface text-xs border-border/70">
                  <div className="flex items-center gap-2 truncate">
                    <Sparkles className="h-3 w-3 shrink-0 text-primary" />
                    <SelectValue
                      placeholder={
                        loadedTemplateSource === 'file'
                          ? loadedTemplateFilename || 'Imported Template'
                          : 'Select Template...'
                      }
                    />
                  </div>
                </SelectTrigger>
                <SelectContent>
                  {templateGroups.map((group) => (
                    <SelectGroup key={group.key}>
                      <SelectLabel className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-widest">
                        <span>{group.label}</span>
                      </SelectLabel>
                      <SelectSeparator className="my-0" />
                      {group.templates.map((template) => (
                        <SelectItem key={template.id} value={template.id}>
                          {template.name} {template.type === 'user' && '(User)'}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  ))}
                </SelectContent>
              </Select>

              <div className="flex items-center gap-1">
                <SimpleTooltip side="bottom" content="New Template">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                    onClick={handleCreateNewTemplate}
                  >
                    <FilePlus2 className="h-4 w-4" />
                  </Button>
                </SimpleTooltip>
                {showTemplateStatus && config && (
                  <SimpleTooltip side="bottom" content="Save Template">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                      onClick={handleSaveTemplate}
                    >
                      <Save className="h-4 w-4" />
                    </Button>
                  </SimpleTooltip>
                )}
                <SimpleTooltip side="bottom" content="Import Template">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                    onClick={handleImportTemplate}
                  >
                    <FolderOpen className="h-4 w-4" />
                  </Button>
                </SimpleTooltip>
              </div>

              {showTemplateStatus ? (
                <Badge
                  variant={status === 'Modified' ? 'secondary' : 'outline'}
                  className={`text-[10px] h-5 ${
                    status === 'Modified'
                      ? 'border-accent-border bg-surface-accent-soft text-primary'
                      : ''
                  }`}
                >
                  {status}
                </Badge>
              ) : null}
            </div>
          </div>
        </div>

        <div className="justify-self-center">
          <div className="flex items-center gap-1 rounded-lg border border-border/70 bg-card/80 p-1 backdrop-blur-sm shadow-lg">
            <SimpleTooltip side="bottom" content="Checkered background">
              <Button
                type="button"
                variant={backgroundMode === 'checker' ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetBackgroundMode('checker')}
              >
                <LayoutGrid className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <SimpleTooltip side="bottom" content="Black background">
              <Button
                type="button"
                variant={backgroundMode === 'black' ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetBackgroundMode('black')}
              >
                <Square className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <SimpleTooltip side="bottom" content="White background">
              <Button
                type="button"
                variant={backgroundMode === 'white' ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetBackgroundMode('white')}
              >
                <Square className="h-4 w-4 fill-[#f4ead2] text-[#f4ead2]" />
              </Button>
            </SimpleTooltip>
            <div className="mx-1 h-5 w-px bg-border/70" />
            <SimpleTooltip side="bottom" content="Zoom out">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={onZoomOut}
              >
                <Minus className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <div className="min-w-14 text-center text-xs font-semibold text-muted-foreground">
              {Math.round(zoomLevel * 100)}%
            </div>
            <SimpleTooltip side="bottom" content="Zoom in">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={onZoomIn}
              >
                <ZoomIn className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <SimpleTooltip side="bottom" content="Reset zoom">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={onResetZoom}
              >
                <RotateCcw className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <div className="mx-1 h-5 w-px bg-border/70" />
            <SimpleTooltip side="bottom" content="Grid">
              <Button
                type="button"
                variant={gridVisible ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetGridVisible(!gridVisible)}
              >
                <Grid3X3 className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            <SimpleTooltip side="bottom" content="Snap to grid">
              <Button
                type="button"
                variant={snapToGrid ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetSnapToGrid(!snapToGrid)}
              >
                <Magnet className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
          </div>
        </div>

        <div className="flex items-center justify-end gap-3 justify-self-end">
          <SimpleTooltip side="bottom" content={renderTooltipContent}>
            <Button
              size="sm"
              className="h-9 bg-primary text-primary-foreground hover:bg-primary/90"
              disabled={renderDisabled}
              onClick={onOpenRenderDialog}
            >
              <Play className="mr-2 h-4 w-4" />
              {renderingVideo ? 'Rendering...' : 'Render'}
            </Button>
          </SimpleTooltip>

          <SimpleTooltip
            side="bottom"
            content={backendStatus !== 'connected' ? 'Backend offline' : null}
          >
            <Button
              variant="outline"
              size="sm"
              className="h-9 gap-2 border-accent-border/70 px-4 text-muted-foreground hover:border-accent-border hover:bg-surface-accent-soft hover:text-foreground"
              disabled={backendStatus !== 'connected'}
              onClick={onOpenDownloads}
            >
              <FolderOpen className="h-3.5 w-3.5" />
              <span>Overlays</span>
            </Button>
          </SimpleTooltip>
        </div>
      </div>
    </header>
  )
}
