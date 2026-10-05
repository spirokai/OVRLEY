/**
 * Batch section of the render dialog — video/output folder pickers and the
 * queued videos with per-item overlay toggles, status, and live progress.
 * Pure presentational - all logic is in useBatchRenderWorkflow.
 */

import { CheckCircle2, FolderOpen, Loader2, Square, Trash2, XCircle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Progress } from '@/components/ui/progress'
import { Switch } from '@/components/ui/switch'
import { formatFps, formatTime } from '../utils/codecUtils'
import { useTranslation } from 'react-i18next'

function StatusIcon({ status }) {
  if (status === 'done') return <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-500" />
  if (status === 'error') return <XCircle className="h-4 w-4 shrink-0 text-red-500" />
  if (status === 'cancelled') return <Square className="h-4 w-4 shrink-0 text-muted-foreground" />
  if (status === 'importing' || status === 'rendering' || status === 'checking')
    return <Loader2 className="h-4 w-4 shrink-0 animate-spin text-primary" />
  return <span className="h-4 w-4 shrink-0" />
}

function FolderPicker({ label, folder, onPick, disabled }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-2">
      <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">{label}</Label>
      <Button
        type="button"
        variant="outline"
        className="h-9 w-full justify-start gap-2 border-border/80 bg-surface-elevated text-xs text-foreground shadow-xs hover:bg-surface-strong"
        onClick={onPick}
        disabled={disabled}
      >
        <FolderOpen className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">{folder || t('render-video.chooseFolder', 'Choose folder...')}</span>
      </Button>
    </div>
  )
}

/**
 * Renders the batch folder pickers and render queue.
 *
 * @param {object} props - Component props.
 * @param {string|null} props.batchVideoFolder - Selected source video folder.
 * @param {string|null} props.batchOutputFolder - Selected output folder.
 * @param {object[]} props.batchQueue - Queued videos.
 * @param {boolean} props.batchRunning - Whether the batch is rendering.
 * @param {string|null} props.batchActiveItemId - Queue item currently rendering.
 * @param {object|null} props.currentItemProgress - Backend progress payload for the active item.
 * @param {function} props.pickVideoFolder - Opens the source folder picker.
 * @param {function} props.pickOutputFolder - Opens the output folder picker.
 * @param {function} props.setBatchItemSkipOverlay - Toggles the activity overlay for an item.
 * @param {function} props.removeBatchQueueItem - Removes an item from the queue.
 * @returns {JSX.Element} Rendered component output.
 */
export default function BatchRenderQueue({
  batchVideoFolder,
  batchOutputFolder,
  batchQueue,
  batchRunning,
  batchActiveItemId,
  currentItemProgress,
  pickVideoFolder,
  pickOutputFolder,
  setBatchItemSkipOverlay,
  removeBatchQueueItem,
}) {
  const { t } = useTranslation()

  return (
    <div className="space-y-4 pt-4">
      <div className="grid grid-cols-2 gap-4">
        <FolderPicker
          label={t('render-video.videoFolder', 'Video folder')}
          folder={batchVideoFolder}
          onPick={pickVideoFolder}
          disabled={batchRunning}
        />
        <FolderPicker
          label={t('render-video.outputFolder', 'Output folder')}
          folder={batchOutputFolder}
          onPick={pickOutputFolder}
          disabled={batchRunning}
        />
      </div>

      <div className="max-h-80 space-y-1 overflow-y-auto rounded-sm border border-border/70 bg-surface p-2">
        {batchQueue.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground">
            {t('render-video.noVideosQueued', 'Choose a video folder to queue videos for batch rendering.')}
          </p>
        ) : (
          batchQueue.map((item) => {
            const isActive = item.id === batchActiveItemId
            return (
              <div key={item.id} className="flex items-center gap-3 rounded-sm px-2 py-2 hover:bg-surface-elevated">
                <StatusIcon status={item.status} />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-medium text-foreground">{item.filename}</p>
                  {item.status === 'checking' ? (
                    <p className="truncate text-[10px] text-muted-foreground">{t('render-video.checkingOverlap', 'Checking overlap...')}</p>
                  ) : null}
                  {item.status === 'error' && item.error ? <p className="truncate text-[10px] text-red-500">{item.error}</p> : null}
                  {isActive && currentItemProgress ? (
                    <>
                      <Progress
                        value={
                          currentItemProgress.current && currentItemProgress.total
                            ? (currentItemProgress.current / currentItemProgress.total) * 100
                            : 0
                        }
                        className="mt-1 h-1"
                      />
                      <p className="mt-1 flex gap-3 text-[10px] tabular-nums text-muted-foreground">
                        <span>
                          {t('render-video.renderFps', 'Render FPS')}: {formatFps(currentItemProgress.rendering_fps)}
                        </span>
                        <span>
                          {t('render-video.estRemaining', 'Est. Remaining')}: {formatTime(currentItemProgress.estimated_seconds_remaining)}
                        </span>
                      </p>
                    </>
                  ) : null}
                </div>
                <Label className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                  {t('render-video.activityOverlay', 'Activity overlay')}
                  <Switch
                    checked={!item.skipOverlay}
                    onCheckedChange={(checked) => setBatchItemSkipOverlay(item.id, !checked)}
                    disabled={batchRunning || item.status === 'checking'}
                  />
                </Label>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-muted-foreground hover:text-red-500"
                  onClick={() => removeBatchQueueItem(item.id)}
                  disabled={batchRunning}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}
