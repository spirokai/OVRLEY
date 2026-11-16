import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { useTranslation } from 'react-i18next'

/**
 * Tells the user once about something that happened to a project from an older
 * OVRLEY version: that it was saved in the current format (with the backup it
 * kept), or that its batch video folder is no longer imported.
 *
 * @param {{open: boolean, title?: string, description?: string, detail?: string, onClose: function}} props Notice.
 */
export function ProjectNoticeDialog({ open, title, description, detail, onClose }) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{title}</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">{description}</DialogDescription>
        <p className="mt-3 text-sm font-medium break-all select-text">{detail}</p>
        <div className="mt-6 flex justify-end">
          <Button type="button" onClick={onClose}>
            {t('projects.ok', 'OK')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
