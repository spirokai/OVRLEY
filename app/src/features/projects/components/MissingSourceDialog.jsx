import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { useTranslation } from 'react-i18next'

export function MissingSourceDialog({ sourceRole, open, onLocate, onLoadAnyway, onCancel }) {
  const { t } = useTranslation()
  const sourceRoleLabel = sourceRole === 'video' ? t('projects.video', 'Video') : t('projects.activity', 'Activity')
  return (
    <Dialog open={open}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">
          {t('projects.sourceNotFound', '{{sourceRole}} source not found', { sourceRole: sourceRoleLabel })}
        </DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {t(
            'projects.missingSourceDescription',
            'The {{sourceRole}} file does not seem to exist. Locate the moved file or cancel without changing the current session.',
            { sourceRole: sourceRoleLabel },
          )}
        </DialogDescription>
        <div className="mt-6 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onCancel}>
            {t('projects.cancel', 'Cancel')}
          </Button>
          <Button type="button" variant="secondary" onClick={onLoadAnyway}>
            {t('projects.loadAnyway', 'Load Anyway')}
          </Button>
          <Button type="button" onClick={onLocate}>
            {t('projects.locate', 'Locate')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
