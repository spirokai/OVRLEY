import { memo } from 'react'
import { CalendarDays, Clock, Loader2, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Switch } from '@/components/ui/switch'
import { formatFps, formatTime } from '../utils/codecUtils'
import { formatProgressPercent } from '../utils/renderPresentation'
import { useTranslation } from 'react-i18next'

/** @param {object} props Queue item, active progress, display mode, and queue actions. @returns {JSX.Element} One presentational queue row. */
function BatchRenderQueueRow({
  item,
  isActive,
  batchRunning,
  showBatchProgress,
  currentItemProgress,
  setBatchItemSkipOverlay,
  removeBatchQueueItem,
}) {
  const { t } = useTranslation()
  const isBlocked = item.status === 'blocked'
  const isDone = item.status === 'succeeded'
  const isRendering = isActive && item.status === 'rendering'
  const isQueued = batchRunning && (item.status === 'pending' || item.status === 'queued')
  const hasProgress = isDone || isRendering || isQueued
  const isLoading = item.status === 'checking' || item.status === 'preparing' || item.status === 'rendering'
  return (
    <div
      key={item.id}
      className={`grid ${showBatchProgress ? 'grid-cols-1' : 'grid-cols-[minmax(0,1fr)_1.75rem_1.75rem]'} items-center gap-3 rounded-sm px-2 py-2 ${isBlocked ? (showBatchProgress ? 'opacity-50' : '[&>*:not(:last-child)]:opacity-50') : 'hover:bg-surface-elevated'}`}
    >
      <div className="min-w-0">
        <div className="flex items-center justify-between gap-3">
          <p className="min-w-0 truncate text-xs font-medium text-foreground" title={item.outputPath ?? item.path}>
            {item.filename}
          </p>
          {hasProgress ? (
            <span
              className={`shrink-0 text-right text-[0.7rem] font-semibold tabular-nums ${isDone ? 'text-green-700' : 'text-muted-foreground/60'}`}
            >
              {isDone
                ? t('render-video.done', 'Done')
                : isQueued
                  ? t('render-video.queued', 'Queued')
                  : `${formatProgressPercent(currentItemProgress.percent)}%`}
            </span>
          ) : null}
        </div>
        {!showBatchProgress && !isBlocked && item.durationLabel ? (
          <p className="mt-0.5 grid grid-cols-[4.5rem_minmax(0,1fr)] items-center gap-3 text-[0.7rem] font-normal leading-tight tabular-nums text-muted-foreground/60">
            <span className="flex items-center gap-1">
              <Clock className="h-3 w-3 shrink-0" aria-hidden="true" />
              <span className="sr-only">{t('toolbar.duration', 'Duration')}: </span>
              <span>{item.durationLabel}</span>
            </span>
            {item.creationDateLabel && (
              <span className="flex min-w-0 items-center gap-1">
                <CalendarDays className="h-3 w-3 shrink-0" aria-hidden="true" />
                <span className="sr-only">{t('toolbar.createdAt', 'Created at')}: </span>
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate">{item.creationDateLabel}</span>
                  {item.creationTimeLabel && <span className="shrink-0">{item.creationTimeLabel}</span>}
                </span>
              </span>
            )}
          </p>
        ) : null}
        {item.status === 'checking' ? (
          <p className="mt-0.5 truncate text-[0.7rem] font-normal leading-tight tabular-nums text-muted-foreground/60">
            {t('render-video.analyzingVideo', 'Analyzing video...')}
          </p>
        ) : null}
        {isBlocked ? (
          <p className="mt-0.5 truncate text-[0.7rem] font-normal leading-tight text-muted-foreground/60" title={item.error}>
            {item.error === 'reinspectionRequired' ? t('render-video.reinspectionRequired') : item.error}
          </p>
        ) : null}
        {item.status === 'failed' && item.error ? <p className="truncate text-[0.7rem] text-red-700">{item.error}</p> : null}
        {item.status === 'preparing' ? <p className="text-[0.7rem] text-muted-foreground/60">{t('render-video.preparingBatch')}</p> : null}
        {item.status === 'cancelled' || item.status === 'unstarted' ? (
          <p className="text-[0.7rem] text-muted-foreground/60">{t(`render-video.${item.status}`)}</p>
        ) : null}
        {hasProgress ? <Progress value={isDone ? 100 : isQueued ? 0 : currentItemProgress.percent} className="mt-2 h-1.5" /> : null}
        {isRendering ? (
          <>
            <p className="mt-1 flex gap-3 text-[0.7rem] tabular-nums text-muted-foreground/60">
              <span>
                {t('render-video.renderFps', 'Render FPS')}: {formatFps(currentItemProgress.renderingFps)}
              </span>
              <span>
                {t('render-video.estRemaining', 'Est. Remaining')}: {formatTime(currentItemProgress.estimatedSecondsRemaining)}
              </span>
            </p>
          </>
        ) : null}
      </div>
      {!showBatchProgress && (
        <>
          <div className="flex h-7 items-center justify-center">
            {isLoading && (
              <span
                role="status"
                aria-label={
                  item.status === 'checking' ? t('render-video.analyzingVideo', 'Analyzing video...') : t('render-video.rendering', 'Rendering...')
                }
              >
                <Loader2 className="h-4 w-4 animate-spin text-primary" />
              </span>
            )}
            <Switch
              aria-label={`${t('render-video.activityOverlay', 'Activity overlay')}: ${item.filename}`}
              checked={!isBlocked && !item.skipOverlay}
              onCheckedChange={(checked) => setBatchItemSkipOverlay(item.id, !checked)}
              disabled={batchRunning || isBlocked}
            />
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground enabled:hover:text-primary disabled:hover:bg-transparent"
            onClick={() => removeBatchQueueItem(item.id)}
            disabled={batchRunning}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </>
      )}
    </div>
  )
}

export default memo(BatchRenderQueueRow)
