/**
 * Center column of the app header — background mode toggle, zoom controls, grid, and snap-to-grid.
 * Pure presentational component.
 */

import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { Film, Grid3X3, Image, LayoutGrid, Magnet, Minus, RotateCcw, Square, ZoomIn } from 'lucide-react'

/**
 * Renders the editor toolbar controls for background mode, zoom, grid, and snap.
 *
 * @param {object} props
 * @param {string} props.backgroundMode - Current background mode ('checker' | 'black' | 'white' | 'video' | 'image').
 * @param {function} props.onSetBackgroundMode - Sets the background mode.
 * @param {string|null} props.importedBackgroundImageFilename - Imported image filename; if set shows image background toggle.
 * @param {string|null} props.importedVideoFilename - Video filename; if set shows video background toggle.
 * @param {number} props.zoomLevel - Current zoom level (0–1 range).
 * @param {function} props.onZoomIn - Increases zoom level.
 * @param {function} props.onZoomOut - Decreases zoom level.
 * @param {function} props.onResetZoom - Resets zoom to default.
 * @param {boolean} props.gridVisible - Whether the grid is visible.
 * @param {function} props.onSetGridVisible - Toggles grid visibility.
 * @param {boolean} props.snapToGrid - Whether snap-to-grid is enabled.
 * @param {function} props.onSetSnapToGrid - Toggles snap-to-grid.
 * @returns {JSX.Element} Rendered component.
 */
export default function EditorToolbar({
  backgroundMode,
  onSetBackgroundMode,
  importedBackgroundImageFilename,
  importedVideoFilename,
  zoomLevel,
  onZoomIn,
  onZoomOut,
  onResetZoom,
  gridVisible,
  onSetGridVisible,
  snapToGrid,
  onSetSnapToGrid,
}) {
  return (
    <div>
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

        {(importedBackgroundImageFilename || importedVideoFilename) && (
          <>
            <div className="mx-1 h-5 w-px bg-border/70" />
            {importedBackgroundImageFilename ? (
              <SimpleTooltip side="bottom" content="Image background">
                <Button
                  type="button"
                  variant={backgroundMode === 'image' ? 'default' : 'ghost'}
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => onSetBackgroundMode('image')}
                >
                  <Image className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
            ) : null}
            <SimpleTooltip side="bottom" content="Video background">
              <Button
                type="button"
                variant={backgroundMode === 'video' ? 'default' : 'ghost'}
                size="icon"
                className="h-8 w-8"
                onClick={() => onSetBackgroundMode('video')}
              >
                <Film className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
          </>
        )}

        <div className="mx-1 h-5 w-px bg-border/70" />
        <SimpleTooltip side="bottom" content="Zoom out">
          <Button type="button" variant="ghost" size="icon" className="h-8 w-8" onClick={onZoomOut}>
            <Minus className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        <div className="min-w-14 text-center text-xs font-semibold text-muted-foreground">{Math.round(zoomLevel * 100)}%</div>
        <SimpleTooltip side="bottom" content="Zoom in">
          <Button type="button" variant="ghost" size="icon" className="h-8 w-8" onClick={onZoomIn}>
            <ZoomIn className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        <SimpleTooltip side="bottom" content="Reset zoom">
          <Button type="button" variant="ghost" size="icon" className="h-8 w-8" onClick={onResetZoom}>
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
  )
}
