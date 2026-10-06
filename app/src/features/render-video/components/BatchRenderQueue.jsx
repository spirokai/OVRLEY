/**
 * Batch folder controls and queued videos with overlay toggles and render progress.
 * Pure presentational - all logic is in useBatchRenderWorkflow.
 */

import { Files, FolderOpen, Loader2, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ButtonGroup } from '@/components/ui/button-group'
import { Label } from '@/components/ui/label'
import { Progress } from '@/components/ui/progress'
import { Switch } from '@/components/ui/switch'
import { formatFps, formatTime } from '../utils/codecUtils'
import { useTranslation } from 'react-i18next'

/**
 * Renders the batch video queue.
 *
 * @param {object} props - Component props.
 * @param {string|null} props.batchVideoFolder - Selected input folder.
 * @param {function} props.pickVideoFolder - Chooses an input folder.
 * @param {function} props.clearBatchQueue - Clears the folder and queue.
 * @param {object[]} props.batchQueue - Queued videos.
 * @param {object[]} props.visibleBatchQueue - Videos included by the display filter.
 * @param {boolean} props.showAllBatchVideos - Whether blocked videos are included.
 * @param {function} props.setShowAllBatchVideos - Changes the display filter.
 * @param {boolean} props.batchRunning - Whether the batch is rendering.
 * @param {boolean} props.showBatchProgress - Whether rendering or finished results are being shown.
 * @param {string|null} props.batchActiveItemId - Queue item currently rendering.
 * @param {object} props.currentItemProgress - Frontend progress for the active item.
 * @param {function} props.setBatchItemSkipOverlay - Toggles the activity overlay for an item.
 * @param {function} props.removeBatchQueueItem - Removes an item from the queue.
 * @returns {JSX.Element} Rendered component output.
 */
export default function BatchRenderQueue({
  batchVideoFolder,
  pickVideoFolder,
  clearBatchQueue,
  batchQueue,
  visibleBatchQueue,
  showAllBatchVideos,
  setShowAllBatchVideos,
  batchRunning,
  showBatchProgress,
  batchActiveItemId,
  currentItemProgress,
  setBatchItemSkipOverlay,
  removeBatchQueueItem,
  batchReviewError,
  refreshInspection,
}) {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-0 min-w-0 flex-col gap-4">
      {!showBatchProgress && (
        <>
          <div className="flex h-7 shrink-0 items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-3">
              <Files className="h-4 w-4 shrink-0 text-primary" />
              <h2 className="truncate text-sm font-semibold text-foreground">{t('render-video.batchSettings', 'Batch Settings')}</h2>
            </div>
            <Label className="flex shrink-0 items-center gap-2 text-[10px] text-muted-foreground">
              {t('render-video.showAll', 'Show all')}
              <Switch aria-label={t('render-video.showAll', 'Show all')} checked={showAllBatchVideos} onCheckedChange={setShowAllBatchVideos} />
            </Label>
          </div>
          <BatchFolderPicker
            label={t('render-video.videoFolder', 'Video folder')}
            folder={batchVideoFolder}
            onPick={pickVideoFolder}
            disabled={batchRunning}
          >
            <Button
              type="button"
              variant="outline"
              className="h-9 border-border/80 bg-surface-elevated text-xs text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
              onClick={clearBatchQueue}
              disabled={batchRunning || (batchVideoFolder === null && batchQueue.length === 0)}
            >
              {t('render-video.clearQueue', 'Clear')}
            </Button>
          </BatchFolderPicker>
          {batchReviewError && (
            <div role="alert" className="shrink-0 space-y-2 text-xs text-red-700">
              <p>{batchReviewError === 'reinspectionRequired' ? t('render-video.reinspectionRequired') : batchReviewError}</p>
              <Button type="button" variant="outline" onClick={refreshInspection}>
                {t('render-video.inspectAgain')}
              </Button>
            </div>
          )}
        </>
      )}
      <div
        className={`min-h-0 flex-1 space-y-1 overflow-y-auto rounded-sm border border-border/70 bg-surface p-2 [scrollbar-gutter:stable] md:contain-size ${showBatchProgress ? 'mt-4' : ''}`}
      >
        {visibleBatchQueue.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground my-auto">
            {batchQueue.length === 0
              ? t('render-video.noVideosQueued', 'Choose a video folder to queue videos for batch rendering.')
              : t('render-video.noRenderableVideos', 'No renderable videos.')}
          </p>
        ) : (
          visibleBatchQueue.map((item) => {
            const isActive = item.id === batchActiveItemId
            const isBlocked = item.status === 'blocked'
            const isDone = item.status === 'succeeded'
            const isRendering = isActive && item.status === 'rendering'
            const isQueued = batchRunning && (item.status === 'pending' || item.status === 'queued')
            const hasProgress = isDone || isRendering || isQueued
            const isLoading = item.status === 'checking' || item.status === 'preparing' || item.status === 'rendering'
            return (
              <div
                key={item.id}
                className={`grid ${showBatchProgress ? 'grid-cols-1' : 'grid-cols-[minmax(0,1fr)_5rem_1.75rem]'} items-center gap-3 rounded-sm px-2 py-2 ${isBlocked ? (showBatchProgress ? 'opacity-50' : '[&>*:not(:last-child)]:opacity-50') : 'hover:bg-surface-elevated'}`}
              >
                <div className="min-w-0">
                  <div className="flex items-center justify-between gap-3">
                    <p className="min-w-0 truncate text-xs font-medium text-foreground" title={item.outputPath ?? item.path}>
                      {item.filename}
                    </p>
                    {hasProgress ? (
                      <span
                        className={`shrink-0 text-right text-[0.7rem] font-semibold tabular-nums ${isDone ? 'text-green-700' : 'text-muted-foreground'}`}
                      >
                        {isDone ? t('render-video.done', 'Done') : isQueued ? t('render-video.queued', 'Queued') : `${currentItemProgress.percent}%`}
                      </span>
                    ) : null}
                  </div>
                  {item.status === 'checking' ? (
                    <p className="truncate text-[10px] text-muted-foreground">{t('render-video.checkingOverlap', 'Checking overlap...')}</p>
                  ) : null}
                  {isBlocked ? (
                    <p className="mt-0.5 truncate text-[10px] font-normal leading-tight text-muted-foreground/70" title={item.error}>
                      {item.error === 'reinspectionRequired' ? t('render-video.reinspectionRequired') : item.error}
                    </p>
                  ) : null}
                  {item.status === 'failed' && item.error ? <p className="truncate text-[10px] text-red-700">{item.error}</p> : null}
                  {item.status === 'preparing' ? <p className="text-[10px] text-muted-foreground">{t('render-video.preparingBatch')}</p> : null}
                  {item.status === 'cancelled' || item.status === 'unstarted' ? (
                    <p className="text-[10px] text-muted-foreground">{t(`render-video.${item.status}`)}</p>
                  ) : null}
                  {hasProgress ? <Progress value={isDone ? 100 : isQueued ? 0 : currentItemProgress.percent} className="mt-2 h-1.5" /> : null}
                  {isRendering ? (
                    <>
                      <p className="mt-1 flex gap-3 text-[10px] tabular-nums text-muted-foreground">
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
                      {isLoading ? (
                        <span
                          role="status"
                          aria-label={
                            item.status === 'checking'
                              ? t('render-video.checkingOverlap', 'Checking overlap...')
                              : t('render-video.rendering', 'Rendering...')
                          }
                        >
                          <Loader2 className="h-4 w-4 animate-spin text-primary" />
                        </span>
                      ) : (
                        <Switch
                          aria-label={`${t('render-video.activityOverlay', 'Activity overlay')}: ${item.filename}`}
                          checked={!isBlocked && !item.skipOverlay}
                          onCheckedChange={(checked) => setBatchItemSkipOverlay(item.id, !checked)}
                          disabled={batchRunning || isBlocked}
                        />
                      )}
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-muted-foreground enabled:hover:text-red-700 disabled:hover:bg-transparent"
                      onClick={() => removeBatchQueueItem(item.id)}
                      disabled={batchRunning}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </>
                )}
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}

/**
 * Presents a batch folder picker with optional joined actions.
 * @param {object} props Folder selection, picker callback, and optional actions.
 * @returns {JSX.Element} Folder field.
 */
export function BatchFolderPicker({ label, folder, onPick, disabled, children }) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0 shrink-0 space-y-2">
      <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">{label}</Label>
      <ButtonGroup className="w-full">
        <Button
          type="button"
          variant="outline"
          className="h-9 min-w-0 flex-1 justify-start gap-2 border-border/80 bg-surface-elevated text-xs text-foreground shadow-xs hover:bg-surface-strong"
          onClick={onPick}
          disabled={disabled}
        >
          <FolderOpen className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{folder || t('render-video.chooseFolder', 'Choose folder...')}</span>
        </Button>
        {children}
      </ButtonGroup>
    </div>
  )
}
