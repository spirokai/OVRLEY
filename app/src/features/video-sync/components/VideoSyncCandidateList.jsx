import { Check, MapPin } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

/**
 * Renders calculated candidates and their calculation lifecycle states.
 *
 * @param {object} props Candidate state and callbacks.
 * @param {object[]} props.candidates Canonical derived candidates.
 * @param {string} props.status Candidate calculation status.
 * @param {string|null} props.error Calculation error.
 * @param {boolean} props.hasSearched Whether a candidate search has run.
 * @param {number} props.appliedOffset Current canonical applied offset.
 * @param {(candidate: object) => void} props.onApply Applies one candidate.
 * @returns {JSX.Element|null} Candidate state or cards.
 */
export function VideoSyncCandidateList({ appliedOffset, candidates, error, hasSearched, onApply, status }) {
  const { t } = useTranslation()

  if (status === 'calculating') {
    return (
      <div className="space-y-2">
        <p className="text-xs text-muted-foreground">{t('videoSync.calculating', 'Calculating landmark matches…')}</p>
        {candidates.length > 0 ? <CandidateCards appliedOffset={appliedOffset} candidates={candidates} disabled onApply={onApply} /> : null}
      </div>
    )
  }

  if (status === 'error') {
    return (
      <div className="space-y-2">
        <p role="alert" className="rounded-sm bg-destructive/10 p-2 text-xs font-medium text-destructive">
          {error}
        </p>
        {candidates.length > 0 ? <CandidateCards appliedOffset={appliedOffset} candidates={candidates} disabled onApply={onApply} /> : null}
      </div>
    )
  }

  if (status === 'stale') {
    return (
      <div className="space-y-2">
        <p className="text-xs font-medium text-amber-400">{t('videoSync.staleCandidates', 'Landmarks changed—run Landmark Sync again')}</p>
        {candidates.length > 0 ? <CandidateCards appliedOffset={appliedOffset} candidates={candidates} disabled onApply={onApply} /> : null}
      </div>
    )
  }

  if (!hasSearched) return null
  if (candidates.length === 0) {
    return <p className="text-xs text-muted-foreground">{t('videoSync.noCandidate', 'No candidate aligns at least two landmarks')}</p>
  }

  return <CandidateCards appliedOffset={appliedOffset} candidates={candidates} onApply={onApply} />
}

function CandidateCards({ appliedOffset, candidates, disabled = false, onApply }) {
  const { t } = useTranslation()

  return (
    <div className="space-y-1.5" role="list" aria-label={t('videoSync.candidates', 'Sync candidates')}>
      {candidates.map((candidate) => {
        const isMapOnly = candidate.variant === 'mapOnly'
        const isMapConflict = candidate.variant === 'mapConflict'
        return (
          <div key={`${candidate.variant}-${candidate.offset}`} role="listitem">
            <Button
              type="button"
              variant={candidate.offset === appliedOffset ? 'default' : 'outline'}
              className="h-auto min-h-14 w-full justify-start px-3 py-2 text-left"
              disabled={disabled}
              aria-label={t('videoSync.applyCandidate', 'Apply offset {{offset}} seconds', { offset: candidate.offset })}
              onClick={() => onApply(candidate)}
            >
              <div className="flex min-w-0 flex-1 items-start gap-2">
                {candidate.offset === appliedOffset ? <Check className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" /> : null}
                {isMapOnly || isMapConflict ? <MapPin className="text-video-sync-location mt-0.5 size-3.5 shrink-0" aria-hidden="true" /> : null}
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs font-semibold">
                    {t('videoSync.offset', 'Offset {{offset}} seconds', { offset: candidate.offset.toFixed(1) })}
                  </div>
                  <div className="mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
                    {candidate.matchScore === null ? null : (
                      <span>{t('videoSync.matchScore', 'Match score: {{score}}', { score: candidate.matchScore })}</span>
                    )}
                    {isMapOnly ? (
                      <span>{t('videoSync.mapOnly', 'Map only')}</span>
                    ) : (
                      <span>
                        {t('videoSync.landmarksMatched', '{{matched}} of {{eligible}} landmarks matched', {
                          matched: candidate.matchedCount,
                          eligible: candidate.eligibleCount,
                        })}
                      </span>
                    )}
                  </div>
                  {isMapConflict ? (
                    <div className="text-video-sync-location mt-1 text-[10px] font-medium">
                      {t('videoSync.mapConflict', 'Map landmark excluded—conflicts with this candidate')}
                    </div>
                  ) : null}
                </div>
              </div>
            </Button>
          </div>
        )
      })}
    </div>
  )
}
