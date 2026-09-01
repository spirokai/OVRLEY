import { Pause, Play, Rewind, RotateCcw, StepBack, StepForward, Volume2, VolumeX, X, ZoomIn, ZoomOut } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTranslation } from 'react-i18next'

/**
 * Presentational toolbar for zoom, fit target, transport, and time display controls.
 *
 * @param {{ toolbar: object }} props Toolbar view model.
 */
export default function PlayerToolbar({ toolbar }) {
  const { t } = useTranslation()
  return (
    <div className="grid w-full grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-4">
      <div className="flex items-center gap-1">
        <SimpleTooltip side="top" content={t('player.zoomOut', 'Zoom out')}>
          <Button type="button" aria-label={t('player.zoomOut', 'Zoom out')} size="toolbar-icon" variant="toolbar" onClick={toolbar.zoomOut}>
            <ZoomOut className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        <SimpleTooltip side="top" content={t('player.zoomIn', 'Zoom in')}>
          <Button type="button" aria-label={t('player.zoomIn', 'Zoom in')} size="toolbar-icon" variant="toolbar" onClick={toolbar.zoomIn}>
            <ZoomIn className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        <SimpleTooltip side="top" content={t('player.resetView', 'Reset view')}>
          <Button
            type="button"
            aria-label={t('player.resetView', 'Reset view')}
            size="toolbar-icon"
            variant="toolbar"
            disabled={toolbar.resetView.disabled}
            onClick={toolbar.resetView.onClick}
          >
            <RotateCcw className="h-3 w-3" />
          </Button>
        </SimpleTooltip>
        <Tabs
          value={toolbar.fitTargets.find((target) => target.isActive)?.id ?? ''}
          onValueChange={(targetId) => toolbar.fitTargets.find((target) => target.id === targetId).onSelect()}
        >
          <TabsList variant="toolbar" className="ml-1">
            {toolbar.fitTargets.map((target) => (
              <TabsTrigger
                key={target.id}
                value={target.id}
                variant="toolbar"
                aria-keyshortcuts={target.id === 'all' ? 'Shift+Z Alt+1' : target.id === 'activity' ? 'Alt+2' : 'Alt+3'}
              >
                {target.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <div className="ml-2 flex items-center gap-1">
          <SimpleTooltip side="top" content={t('player.setExportStartAtPlayhead', 'Set export start at playhead')}>
            <Button
              type="button"
              aria-label={t('player.setExportStartAtPlayhead', 'Set export start at playhead')}
              size="toolbar-icon"
              variant="ghost"
              disabled={toolbar.exportRange.isDisabled}
              onClick={toolbar.exportRange.setStart}
              aria-keyshortcuts="I"
            >
              <span aria-hidden="true" className="font-mono text-base font-normal">
                [
              </span>
            </Button>
          </SimpleTooltip>
          <SimpleTooltip side="top" content={t('player.setExportEndAtPlayhead', 'Set export end at playhead')}>
            <Button
              type="button"
              aria-label={t('player.setExportEndAtPlayhead', 'Set export end at playhead')}
              size="toolbar-icon"
              variant="ghost"
              disabled={toolbar.exportRange.isDisabled}
              onClick={toolbar.exportRange.setEnd}
              aria-keyshortcuts="O"
            >
              <span aria-hidden="true" className="font-mono text-base font-normal">
                ]
              </span>
            </Button>
          </SimpleTooltip>
          {toolbar.exportRange.isCustom ? (
            <div className="ml-1 flex items-center gap-0.5 text-orange-400/90">
              <span className="text-xs font-medium tabular-nums">{toolbar.exportRange.label}</span>
              <SimpleTooltip side="top" content={t('player.clearCustomExportRange', 'Clear custom export range')}>
                <Button
                  type="button"
                  aria-label={t('player.clearCustomExportRange', 'Clear custom export range')}
                  size="toolbar-icon"
                  variant="ghost"
                  className="text-orange-400/90 hover:text-orange-300"
                  onClick={toolbar.exportRange.clear}
                  aria-keyshortcuts="Mod+X"
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </SimpleTooltip>
            </div>
          ) : null}
        </div>
      </div>

      <div className="flex items-center gap-1 rounded-xs border border-border/30 p-0.5 shadow-sm">
        <Button
          type="button"
          aria-label={t('player.rewindToStart', 'Rewind to start')}
          size="toolbar-icon"
          variant="toolbar"
          disabled={toolbar.transport.isDisabled}
          onClick={toolbar.transport.resetToStart}
          aria-keyshortcuts="Home"
        >
          <Rewind className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          aria-label={t('player.stepBack', 'Step back')}
          size="toolbar-icon"
          variant="toolbar"
          disabled={toolbar.transport.isDisabled}
          onClick={toolbar.transport.stepBackward}
          aria-keyshortcuts="ArrowLeft"
        >
          <StepBack className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          aria-label={toolbar.transport.isPlaying ? 'Pause' : 'Play'}
          size="toolbar-icon"
          variant={toolbar.transport.isPlaying ? 'secondary' : 'default'}
          disabled={toolbar.transport.isDisabled}
          onClick={toolbar.transport.isPlaying ? toolbar.transport.pause : toolbar.transport.play}
          aria-keyshortcuts="Space"
        >
          {toolbar.transport.isPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" strokeWidth={2} />}
        </Button>
        <Button
          type="button"
          aria-label={t('player.stepForward', 'Step forward')}
          size="toolbar-icon"
          variant="toolbar"
          disabled={toolbar.transport.isDisabled}
          onClick={toolbar.transport.stepForward}
          aria-keyshortcuts="ArrowRight"
        >
          <StepForward className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          aria-label={t('player.rewindToEnd', 'Rewind to end')}
          size="toolbar-icon"
          variant="toolbar"
          disabled={toolbar.transport.isDisabled}
          onClick={toolbar.transport.jumpToEnd}
          aria-keyshortcuts="End"
        >
          <Rewind className="h-3.5 w-3.5 rotate-180" />
        </Button>
      </div>

      <div className="flex w-48 shrink-0 items-center justify-end justify-self-end gap-4">
        {toolbar.hasVideo ? (
          <Button
            type="button"
            aria-label={toolbar.isMuted ? 'Unmute video' : 'Mute video'}
            aria-pressed={toolbar.isMuted}
            variant="ghost"
            size="toolbar-icon"
            onClick={toolbar.toggleMute}
            aria-keyshortcuts="M"
          >
            {toolbar.isMuted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
          </Button>
        ) : null}
        <span className="text-xs font-medium tabular-nums text-muted-foreground">
          {toolbar.timeLabel.current} / {toolbar.timeLabel.total}
        </span>
      </div>
    </div>
  )
}
