import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useTranslation } from 'react-i18next'

/**
 * Asks where a clip goes when its recording time cannot be determined.
 */
export function VideoPlacementDialog({ open, videoName, candidates, anchorVideoId, side, onAnchorChange, onSideChange, onConfirm, onSkip }) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onSkip()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{t('video-preview.placementTitle', 'Place video')}</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {t('video-preview.placementDescription', {
            defaultValue: 'The recording time of {{name}} could not be determined. Choose where it goes on the timeline.',
            name: videoName,
          })}
        </DialogDescription>
        <div className="mt-4 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-3">
          <Select value={side} onValueChange={onSideChange}>
            <SelectTrigger className="w-28" aria-label={t('video-preview.placementSide', 'Side')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="before">{t('video-preview.placementBefore', 'Before')}</SelectItem>
              <SelectItem value="after">{t('video-preview.placementAfter', 'After')}</SelectItem>
            </SelectContent>
          </Select>
          <Select value={anchorVideoId ?? undefined} onValueChange={onAnchorChange}>
            <SelectTrigger className="min-w-0" aria-label={t('video-preview.placementAnchor', 'Video')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {candidates.map((candidate) => (
                <SelectItem key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <p className="mt-2 text-xs text-muted-foreground">
          {t('video-preview.placementSkipHint', 'If you skip, the video starts at the beginning of the timeline.')}
        </p>
        <div className="mt-6 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onSkip}>
            {t('video-preview.placementSkip', 'Skip')}
          </Button>
          <Button type="button" onClick={onConfirm} disabled={anchorVideoId === null}>
            {t('video-preview.placementConfirm', 'Place')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
