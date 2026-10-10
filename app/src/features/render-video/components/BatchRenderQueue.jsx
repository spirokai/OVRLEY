/**
 * Batch folder controls and queued videos with overlay toggles and render progress.
 * Pure presentational - inspection and execution are owned by their hooks.
 */

import { Files, FolderOpen, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ButtonGroup } from '@/components/ui/button-group'
import { HelpTooltip } from '@/components/ui/help-tooltip'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import BatchRenderQueueRow from './BatchRenderQueueRow'
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
  batchInspecting,
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
              <div className="flex min-w-0 items-center gap-1">
                <h2 className="truncate text-sm font-semibold text-foreground">{t('render-video.batchSettings', 'Batch Settings')}</h2>
                <HelpTooltip className="-mt-0.5 self-start" content={t('render-video.batchAutoSyncRequirement')} />
              </div>
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
            onRefresh={refreshInspection}
            refreshDisabled={batchInspecting}
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
            </div>
          )}
        </>
      )}
      <div
        className={`min-h-0 flex-1 space-y-1 overflow-y-auto rounded-sm border p-2 [scrollbar-gutter:stable] md:contain-size ${showBatchProgress ? 'mt-4 border-border/25 bg-transparent' : 'border-border/70 bg-surface'}`}
      >
        {visibleBatchQueue.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground my-auto">
            {batchQueue.length === 0
              ? t('render-video.noVideosQueued', 'Choose a video folder to queue videos for batch rendering.')
              : t('render-video.noRenderableVideos', 'No renderable videos.')}
          </p>
        ) : (
          visibleBatchQueue.map((item) => (
            <BatchRenderQueueRow
              key={item.id}
              item={item}
              isActive={item.id === batchActiveItemId}
              batchRunning={batchRunning}
              showBatchProgress={showBatchProgress}
              currentItemProgress={item.id === batchActiveItemId ? currentItemProgress : null}
              setBatchItemSkipOverlay={setBatchItemSkipOverlay}
              removeBatchQueueItem={removeBatchQueueItem}
            />
          ))
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
export function BatchFolderPicker({ label, folder, onPick, disabled, onRefresh, refreshDisabled, children }) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0 shrink-0 space-y-2">
      <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">{label}</Label>
      <ButtonGroup className="w-full">
        <div className="relative min-w-0 flex-1">
          <Button
            type="button"
            variant="outline"
            className={`h-9 w-full min-w-0 justify-start gap-2 border-border/80 bg-surface-elevated text-xs text-foreground shadow-xs hover:bg-surface-strong ${children ? 'rounded-r-none' : ''} ${onRefresh ? 'has-[>svg]:pr-9' : ''}`}
            onClick={onPick}
            disabled={disabled}
          >
            <FolderOpen className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{folder || t('render-video.chooseFolder', 'Choose folder...')}</span>
          </Button>
          {onRefresh && (
            <button
              type="button"
              className="absolute inset-y-0 right-0 flex w-9 cursor-pointer items-center justify-center rounded-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50"
              aria-label={t('render-video.inspectAgain')}
              onClick={onRefresh}
              disabled={disabled || refreshDisabled || folder === null}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
        {children}
      </ButtonGroup>
    </div>
  )
}
