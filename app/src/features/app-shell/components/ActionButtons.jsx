/**
 * Right column of the app header — render video button and open overlays button.
 * Pure presentational component.
 */

import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { FolderOpen, ImageDown, Play } from 'lucide-react'
import WindowControls from './WindowControls'
import { useTranslation } from 'react-i18next'

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
 * @param {function} props.onOpenOutputDirectory - Opens the render output folder.
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
  onOpenOutputDirectory,
}) {
  const { t } = useTranslation()
  return (
    <div className="flex min-w-fit items-center justify-end gap-3">
      <div className="flex items-center gap-4">
        <SimpleTooltip side="bottom" content={backendStatus !== 'connected' ? t('app-shell.backendOffline', 'Backend offline') : null}>
          <Button
            variant="outline"
            size="sm"
            className="h-9 gap-2 border-accent-border/70 px-4 text-muted-foreground hover:border-accent-border hover:bg-surface-accent-soft hover:text-foreground"
            disabled={backendStatus !== 'connected'}
            onClick={onOpenOutputDirectory}
            aria-keyshortcuts="Mod+Shift+E"
          >
            <FolderOpen className="h-3.5 w-3.5" />
            <span>{t('app-shell.overlays', 'Overlays')}</span>
          </Button>
        </SimpleTooltip>
      </div>
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
      <SimpleTooltip side="bottom" content={renderTooltipContent} className="pr-4">
        <Button
          size="sm"
          className="h-9 bg-primary text-primary-foreground hover:bg-primary/90"
          disabled={renderDisabled}
          onClick={onOpenRenderDialog}
          aria-keyshortcuts="Mod+E"
        >
          <Play className="mr-2 h-4 w-4" />
          {renderingVideo ? t('render-video.renderingFrames', 'Rendering frames...') : t('render-video.startRender', 'Start Render')}
        </Button>
      </SimpleTooltip>
      <WindowControls />
    </div>
  )
}
