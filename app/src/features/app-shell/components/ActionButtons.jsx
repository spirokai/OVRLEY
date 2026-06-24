/**
 * Right column of the app header — render video button and open overlays button.
 * Pure presentational component.
 */

import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { FolderOpen, ImageDown, Play } from 'lucide-react'
import WindowControls from './WindowControls'

/**
 * Renders the render and overlays action buttons.
 *
 * @param {object} props
 * @param {function} props.onOpenRenderDialog - Opens the render video dialog.
 * @param {function|undefined} props.onRenderPreviewFrame - Renders the current playhead as a transparent PNG.
 * @param {boolean} props.renderDisabled - Whether the render button is disabled.
 * @param {boolean|undefined} props.renderPreviewFrameDisabled - Whether the preview-frame render button is disabled.
 * @param {string|null} props.renderTooltipContent - Tooltip text for the render button.
 * @param {boolean} props.renderingVideo - Whether a render is in progress.
 * @param {string} props.backendStatus - Current backend connection status.
 * @param {function} props.onOpenDownloads - Opens the downloads/output folder.
 * @returns {JSX.Element} Rendered component.
 */
export default function ActionButtons({
  onOpenRenderDialog,
  onRenderPreviewFrame,
  renderDisabled,
  renderPreviewFrameDisabled,
  renderTooltipContent,
  renderingVideo,
  backendStatus,
  onOpenDownloads,
}) {
  return (
    <div className="flex min-w-fit items-center justify-end gap-3">
      <SimpleTooltip side="bottom" content={backendStatus !== 'connected' ? 'Backend offline' : null}>
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
      {onRenderPreviewFrame ? (
        <Button
          variant="outline"
          size="sm"
          className="h-9 gap-2 border-accent-border/70 px-4 text-muted-foreground hover:border-accent-border hover:bg-surface-accent-soft hover:text-foreground"
          disabled={renderPreviewFrameDisabled}
          onClick={onRenderPreviewFrame}
        >
          <ImageDown className="h-3.5 w-3.5" />
          <span>PNG</span>
        </Button>
      ) : null}
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
      <WindowControls />
    </div>
  )
}
