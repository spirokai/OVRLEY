import { ChevronsRight, Download } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { Progress } from '@/components/ui/progress'
import { Trans, useTranslation } from 'react-i18next'

/**
 * Presents update state without owning updater side effects.
 *
 * @param {object} props - Controlled update dialog props.
 * @param {boolean} props.open - Whether the dialog is visible.
 * @param {string} props.phase - Current updater phase.
 * @param {string|null} props.version - Available update version.
 * @param {object|null} props.progress - Download progress state.
 * @param {number} props.progressPercent - Validated determinate progress percentage.
 * @param {string} props.progressLabel - Formatted determinate progress label.
 * @param {function} props.onUpdateNow - Starts the update installation.
 * @param {function} props.onLater - Dismisses the available update.
 * @param {function} props.onClose - Closes a failed update dialog.
 * @returns {JSX.Element|null} Rendered dialog or null when no update is active.
 */
export default function UpdatePromptDialog({ open, phase, version, progress, progressPercent, progressLabel, onUpdateNow, onLater, onClose }) {
  const { t } = useTranslation()
  const currentVersion = import.meta.env.VITE_OVRLEY_VERSION?.trim() || '0.00.0'

  const downloading = phase === 'downloading'
  const failed = phase === 'failed'
  const determinate = progress?.mode === 'determinate'

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && !downloading && onClose()}>
      <DialogContent
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className="w-full max-w-md rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <div className="flex items-center gap-3">
          <Download className="h-4 w-4 text-primary" />
          <DialogTitle className="text-sm font-semibold text-foreground">
            {failed
              ? t('app-update.updateFailed', 'Update failed')
              : downloading
                ? t('app-update.downloadingUpdate', 'Downloading update')
                : t('app-update.updateAvailable', 'Update available')}
          </DialogTitle>
        </div>
        <DialogDescription className="mt-6 normal-case text-[0.9rem] font-light leading-5 text-muted-foreground">
          {failed ? (
            t('app-update.continueWithInstalledVersion', 'The installed version remains available. You can close this dialog and continue working.')
          ) : downloading ? (
            <span className="flex items-center justify-center gap-4 text-2xl font-bold text-foreground">
              <span>{currentVersion}</span>
              <ChevronsRight className="h-6 w-6 shrink-0 text-primary" aria-hidden="true" />
              <span>{version}</span>
            </span>
          ) : (
            <Trans
              i18nKey="app-update.versionNowAvailable"
              defaults="OVRLEY <0>{{version}}</0> is now available."
              values={{ version: version ?? '' }}
              components={[<span key="0" className="text-foreground font-bold" />]}
            />
          )}
        </DialogDescription>

        {downloading ? (
          <div className="mt-3 space-y-2">
            <p className="text-end text-xs text-muted-foreground tabular-nums">
              {determinate ? progressLabel : t('app-update.downloadingUpdate', 'Downloading update...')}
            </p>
            {determinate ? (
              <Progress value={progressPercent} aria-label={t('app-update.updateDownloadProgress', 'Update download progress')} />
            ) : (
              <div className="bg-primary/20 h-2 w-full overflow-hidden rounded-full">
                <div className="bg-primary h-full w-1/3 animate-pulse rounded-full" />
              </div>
            )}
          </div>
        ) : null}

        <div className="mt-6 flex items-center justify-end gap-3">
          {failed ? (
            <Button
              type="button"
              variant="outline"
              className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
              onClick={onClose}
            >
              {t('app-update.close', 'Close')}
            </Button>
          ) : null}
          {!downloading && !failed ? (
            <Button
              type="button"
              variant="outline"
              className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
              onClick={onLater}
            >
              {t('app-update.later', 'Later')}
            </Button>
          ) : null}
          {!downloading && !failed ? (
            <Button type="button" className="bg-primary text-primary-foreground hover:bg-primary/90" onClick={onUpdateNow}>
              {t('app-update.updateNow', 'Update now')}
            </Button>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
