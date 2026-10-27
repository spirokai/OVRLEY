/**
 * Imported videos of an export, with overlay toggles, the reasons a combined file
 * cannot be rendered, and render progress.
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
 * Renders the queue of imported videos.
 *
 * @param {object} props - Component props.
 * @param {object[]} props.batchQueue - Imported videos in timeline order.
 * @param {object[]} props.visibleBatchQueue - Videos included by the display filter.
 * @param {boolean} props.showAllBatchVideos - Whether blocked videos are included.
 * @param {function} props.setShowAllBatchVideos - Changes the display filter.
 * @param {boolean} props.batchRunning - Whether the batch is rendering.
 * @param {boolean} props.showBatchProgress - Whether rendering or finished results are being shown.
 * @param {string|null} props.batchActiveItemId - Queue item currently rendering.
 * @param {object} props.currentItemProgress - Frontend progress for the active item.
 * @param {function} props.setBatchItemSkipOverlay - Toggles the activity overlay for an item.
 * @param {string[]} props.combinedBlockers - Reasons the combined file cannot be rendered.
 * @returns {JSX.Element} Rendered component output.
 */
export default function BatchRenderQueue({
  batchQueue,
  visibleBatchQueue,
  showAllBatchVideos,
  setShowAllBatchVideos,
  batchRunning,
  showBatchProgress,
  batchActiveItemId,
  currentItemProgress,
  setBatchItemSkipOverlay,
  batchReviewError,
  batchInspecting,
  refreshInspection,
  combinedBlockers,
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
                <h2 className="truncate text-sm font-semibold text-foreground">{t('render-video.videos', 'Videos')}</h2>
                <HelpTooltip className="-mt-0.5 self-start" content={t('render-video.batchAutoSyncRequirement')} />
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-3">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-muted-foreground hover:text-foreground"
                aria-label={t('render-video.inspectAgain')}
                onClick={refreshInspection}
                disabled={batchRunning || batchInspecting || batchQueue.length === 0}
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
              <Label className="flex items-center gap-2 text-[0.741rem] text-muted-foreground">
                {t('render-video.showAll', 'Show all')}
                <Switch aria-label={t('render-video.showAll', 'Show all')} checked={showAllBatchVideos} onCheckedChange={setShowAllBatchVideos} />
              </Label>
            </div>
          </div>
          <BatchReviewAlerts combinedBlockers={combinedBlockers} batchReviewError={batchReviewError} />
        </>
      )}
      <div
        className={`min-h-0 flex-1 space-y-1 overflow-y-auto rounded-sm border p-2 [scrollbar-gutter:stable] md:contain-size ${showBatchProgress ? 'mt-4 border-border/25 bg-transparent' : 'border-border/70 bg-surface'}`}
      >
        {visibleBatchQueue.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground my-auto">
            {batchQueue.length === 0
              ? t('render-video.noVideosQueued', 'Import videos to render them.')
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
            />
          ))
        )}
      </div>
    </div>
  )
}

/**
 * Reasons the reviewed videos cannot be rendered: combined-file blockers and inspection errors.
 * @param {{ combinedBlockers: string[], batchReviewError: string|null }} props Review problems.
 * @returns {JSX.Element} Alerts, empty without problems.
 */
export function BatchReviewAlerts({ combinedBlockers, batchReviewError }) {
  const { t } = useTranslation()
  return (
    <>
      {combinedBlockers.length > 0 && (
        <ul
          role="alert"
          aria-label={t('render-video.combinedUnavailableTitle', 'One video file cannot be rendered')}
          className="shrink-0 space-y-1 text-xs text-red-700"
        >
          {combinedBlockers.map((blocker) => (
            <li key={blocker}>{blocker}</li>
          ))}
        </ul>
      )}
      {batchReviewError && (
        <div role="alert" className="shrink-0 space-y-2 text-xs text-red-700">
          <p>{batchReviewError === 'reinspectionRequired' ? t('render-video.reinspectionRequired') : batchReviewError}</p>
        </div>
      )}
    </>
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
      <Label className="text-[0.741rem] font-bold uppercase tracking-widest text-muted-foreground">{label}</Label>
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
