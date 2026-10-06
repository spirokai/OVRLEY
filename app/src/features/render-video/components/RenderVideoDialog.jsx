/**
 * Renders the render video dialog portion of the application interface.
 * Renders either the current video or, in batch mode, every video in a folder.
 * Pure presentational - all logic is in useRenderVideoDialogState.
 */

import { AlertTriangle, Loader2, Play } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import BatchRenderQueue from './BatchRenderQueue'
import RenderExportSettings, { RenderTargetTabs } from './RenderExportSettings'
import RenderProgressPanel from './RenderProgressPanel'
import useRenderVideoDialogState from '../hooks/useRenderVideoDialogState'
import { useTranslation } from 'react-i18next'

/**
 * Renders the render video dialog component.
 *
 * @param {object} props - Component props.
 * @param {*} props.phase - Value for phase.
 * @param {*} props.settings - Value for settings.
 * @param {*} props.onSettingsChange - Callback invoked to settings change.
 * @param {*} props.onClose - Callback invoked to close.
 * @param {*} props.onConfirm - Callback invoked to confirm.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RenderVideoDialog(props) {
  const { t } = useTranslation()
  const ctx = useRenderVideoDialogState(props)

  if (ctx.phase === 'closed') {
    return <Dialog open={false} />
  }

  if (!ctx.settings) {
    throw new Error('Render settings are required while the render dialog is open')
  }

  const hasBlockingResolutionMismatch = ctx.hasImportedVideo && ctx.resolutionMismatch

  return (
    <Dialog
      open
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          ctx.onClose()
        }
      }}
    >
      <DialogContent
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className={`w-full rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50 ${ctx.isBatchTarget ? 'max-w-6xl' : 'max-w-xl'}`}
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (ctx.isProgress || ctx.submissionPending || ctx.batchRunning) {
            event.preventDefault()
          }
        }}
        onPointerDownOutside={(event) => {
          if (ctx.isProgress || ctx.submissionPending || ctx.batchRunning) {
            event.preventDefault()
          }
        }}
      >
        {ctx.isProgress ? (
          <>
            <DialogTitle className="sr-only">{t('render-video.exportingOverlay', 'Exporting Overlay')}</DialogTitle>
            <RenderProgressPanel
              renderProgress={ctx.renderProgress}
              renderSummaryItems={ctx.renderSummaryItems}
              onCancel={ctx.handleCancel}
              isCancelling={ctx.isCancelling}
            />
          </>
        ) : hasBlockingResolutionMismatch ? (
          <div className="space-y-12 p-3">
            <DialogTitle className="sr-only">{t('render-video.videoResolutionMismatch', 'Video resolution mismatch')}</DialogTitle>
            <RenderTargetTabs renderTarget={ctx.settings.renderTarget} onRenderTargetChange={ctx.handleRenderTargetChange} />
            <div className="space-y-8">
              <div className="flex items-start gap-3">
                <AlertTriangle className="mt-0.5 h-10 w-10 shrink-0 text-red-500" />
                <p className="pl-2 font-bold text-sm leading-normal pt-1 text-red-500">
                  {t(
                    'render-video.videoResolutionMismatchMessage',
                    'Overlay resolution ({{overlayResolution}}) must match imported video ({{videoResolution}}).',
                    {
                      overlayResolution: `${ctx.config?.scene?.width}x${ctx.config?.scene?.height}`,
                      videoResolution: `${ctx.importedVideoResolution?.width}x${ctx.importedVideoResolution?.height}`,
                    },
                  )}
                </p>
              </div>
              <p className="text-sm leading-4 text-muted-foreground text-justify">
                {t(
                  'render-video.resolutionMismatchInstructions',
                  'This is necessary to properly export the overlay. Please change the overlay resolution in the sidebar settings or pick a different template.',
                )}
              </p>
            </div>
            <div className="flex justify-end">
              <Button
                type="button"
                variant="outline"
                className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                onClick={ctx.onClose}
              >
                {t('render-video.cancel', 'Cancel')}
              </Button>
            </div>
          </div>
        ) : (
          <div className={`grid gap-12 ${ctx.isBatchTarget ? 'md:grid-cols-2' : 'grid-cols-1'}`}>
            <div className={ctx.showBatchProgress ? 'min-w-0 md:flex md:h-150 md:flex-col md:justify-center md:overflow-y-auto' : 'min-w-0'}>
              {ctx.showBatchProgress ? (
                <>
                  <DialogTitle className="sr-only">{t('render-video.exportingOverlay', 'Exporting Overlay')}</DialogTitle>
                  <RenderProgressPanel
                    renderProgress={ctx.batchProgress}
                    renderSummaryItems={ctx.renderSummaryItems}
                    finished={ctx.batchFinished}
                    batchSnapshot={ctx.batchSnapshot}
                  />
                </>
              ) : (
                <RenderExportSettings {...ctx} />
              )}
            </div>

            {ctx.isBatchTarget && (
              <>
                <BatchRenderQueue {...ctx} />
                <div className="flex items-center justify-end gap-3 md:col-span-2">
                  <div className="flex items-center gap-3">
                    <Button
                      type="button"
                      variant="outline"
                      className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                      onClick={ctx.batchRunning ? ctx.handleCancel : ctx.onClose}
                      disabled={ctx.batchSubmissionPending || ctx.isCancelling || ctx.batchSnapshot?.phase === 'cancelling'}
                    >
                      {ctx.isCancelling || ctx.batchSnapshot?.phase === 'cancelling'
                        ? t('render-video.cancelling', 'Cancelling...')
                        : ctx.batchRunning
                          ? t('render-video.cancel', 'Cancel')
                          : t('render-video.close', 'Close')}
                    </Button>
                    {!ctx.batchFinished && (
                      <Button
                        type="button"
                        className="bg-primary text-primary-foreground hover:bg-primary/90"
                        onClick={ctx.onConfirm}
                        disabled={ctx.batchStartDisabled}
                      >
                        {ctx.batchRunning ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                        {ctx.batchRunning ? t('render-video.rendering', 'Rendering...') : t('render-video.startBatchRender', 'Start Batch Render')}
                      </Button>
                    )}
                  </div>
                </div>
              </>
            )}
          </div>
        )}
      </DialogContent>
      <OverwriteConfirmDialog {...ctx} />
    </Dialog>
  )
}

function OverwriteConfirmDialog({ overwriteOpen, pendingOverwritePath, onOverwriteConfirm, onOverwriteCancel }) {
  const { t } = useTranslation()
  return (
    <Dialog open={Boolean(overwriteOpen)} onOpenChange={(open) => !open && onOverwriteCancel?.()}>
      <DialogContent
        overlayClassName="absolute inset-0 z-130 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className="w-full max-w-md rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl"
      >
        <DialogTitle className="text-sm font-semibold text-foreground">
          {t('render-video.overwriteExistingFile', 'Overwrite existing file?')}
        </DialogTitle>
        <p className="mt-3 break-all text-xs text-muted-foreground">{pendingOverwritePath}</p>
        <div className="mt-6 flex justify-end gap-3">
          <Button type="button" variant="outline" onClick={onOverwriteCancel}>
            {t('render-video.cancel', 'Cancel')}
          </Button>
          <Button type="button" onClick={onOverwriteConfirm}>
            {t('render-video.overwrite', 'Overwrite')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
