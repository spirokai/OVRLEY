import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

/** @param {{candidate: object, onApply: function}} props - Prepared candidate presentation. @returns {JSX.Element} Candidate evidence and explicit apply action. */
function CandidateCard({ candidate, onApply }) {
  return (
    <div className="flex flex-col gap-2 rounded border p-3 text-xs">
      <Button
        size="sm"
        variant={candidate.applied ? 'default' : 'outline'}
        disabled={!candidate.accepted}
        onClick={() => onApply(candidate.offset_seconds)}
      >
        {candidate.action}
      </Button>
      <p>{candidate.evidence}</p>
      <p>{candidate.support}</p>
      <ul className="space-y-1 text-muted-foreground">
        {candidate.sections.map((section) => (
          <li key={section.video_start_seconds}>{section.label}</li>
        ))}
      </ul>
      {candidate.rejection_reasons.length > 0 && (
        <ul className="space-y-1 text-muted-foreground">
          {candidate.rejection_reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** @param {{sync: object}} props - Managed visual analysis presentation. @returns {JSX.Element} Sync Doctor drawer. */
export function SyncDoctorDrawerContent({ sync }) {
  const { t } = useTranslation()
  return (
    <div className="relative flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      <p className="text-xs text-muted-foreground">{t('syncDoctor.description')}</p>
      {!sync.ready && <p className="text-xs text-muted-foreground">{t('syncDoctor.inputsRequired')}</p>}
      <Button onClick={sync.start} disabled={!sync.ready || sync.busy}>
        {t('syncDoctor.analyze')}
      </Button>
      {sync.statusLabel && (
        <div role="status" aria-live="polite" className="flex flex-col gap-2 rounded border bg-card p-3 text-sm">
          {sync.busy && <LoaderCircle className="size-5 animate-spin" />}
          <span>{sync.statusLabel}</span>
          {sync.progressLabel && <span className="text-xs text-muted-foreground">{sync.progressLabel}</span>}
          {sync.busy && (
            <Button variant="outline" size="sm" onClick={sync.cancel}>
              {t('syncDoctor.cancel')}
            </Button>
          )}
        </div>
      )}
      {sync.error && (
        <p role="alert" className="text-sm text-destructive">
          {sync.error}
        </p>
      )}
      {sync.status === 'no_match' && <p className="text-xs text-muted-foreground">{t('syncDoctor.manualGuidance')}</p>}
      {sync.candidates.map((candidate) => (
        <CandidateCard key={candidate.offset_seconds} candidate={candidate} onApply={sync.apply} />
      ))}
    </div>
  )
}
