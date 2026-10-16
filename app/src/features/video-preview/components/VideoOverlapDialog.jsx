import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { useTranslation } from 'react-i18next'

/**
 * Warns that imported videos overlap in time.
 */
export function VideoOverlapDialog({ open, overlaps, onClose }) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{t('video-preview.overlapTitle', 'Videos overlap')}</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {t(
            'video-preview.overlapDescription',
            'These videos cover the same time. One combined export is not possible until you move a video or remove the overlapping part.',
          )}
        </DialogDescription>
        <ul className="mt-4 max-h-[50vh] space-y-2 overflow-y-auto thin-scrollbar">
          {overlaps.map((overlap) => (
            <li key={overlap.key} className="text-xs">
              <span className="font-medium break-all text-foreground">
                {t('video-preview.overlapPair', {
                  defaultValue: '{{earlier}} and {{later}}',
                  earlier: overlap.earlierName,
                  later: overlap.laterName,
                })}
              </span>
              <span className="block text-muted-foreground">
                {t('video-preview.overlapSeconds', { defaultValue: 'Overlap: {{seconds}} s', seconds: overlap.seconds })}
              </span>
            </li>
          ))}
        </ul>
        <div className="mt-6 flex justify-end">
          <Button type="button" onClick={onClose}>
            {t('video-preview.ok', 'OK')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
