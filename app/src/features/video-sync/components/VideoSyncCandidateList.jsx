import { Check, MapPin } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { formatClockDuration } from '@/lib/time-format'
import { getVideoSyncMatchScoreColor } from '../utils/candidatePresentation'

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
        const isApplied = candidate.offset === appliedOffset
        const isMapOnly = candidate.variant === 'mapOnly'
        const isMapConflict = candidate.variant === 'mapConflict'
        const scoreColor = getVideoSyncMatchScoreColor(candidate.matchScore)
        return (
          <div key={`${candidate.variant}-${candidate.offset}`} role="listitem">
            <Button
              type="button"
              variant={isApplied ? 'default' : 'ghost'}
              className={`relative h-auto min-h-14 w-full rounded-xs border border-border/70 px-3 py-2 pr-10 text-left ${
                isApplied ? 'bg-primary text-primary-foreground hover:bg-primary/90' : 'bg-surface-elevated/70 hover:bg-surface-elevated'
              }`}
              disabled={disabled}
              aria-label={t('videoSync.applyCandidate', 'Apply offset {{offset}} seconds', {
                offset: formatClockDuration(candidate.offset),
              })}
              onClick={() => onApply(candidate)}
            >
              <div className="flex min-w-0 flex-1">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    {isMapOnly || isMapConflict ? <MapPin className="size-3.5 shrink-0 text-video-sync-location" aria-hidden="true" /> : null}
                    <span className={`text-base font-bold tabular-nums ${isApplied ? 'text-primary-foreground' : 'text-foreground'}`}>
                      {formatClockDuration(candidate.offset)}
                    </span>
                  </div>
                  <div
                    className={`mt-0.5 text-[10px] text-muted-foreground ${isApplied ? 'text-primary-foreground/60' : 'text-muted-foreground/60'}`}
                  >
                    {isMapOnly ? (
                      <span>{t('videoSync.mapOnly', 'Map only')}</span>
                    ) : (
                      <span>
                        {t('videoSync.landmarksMatched', '{{matched}} / {{eligible}} landmarks matched', {
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
                {candidate.matchScore === null ? null : (
                  <span className="absolute right-3 top-2 text-sm font-semibold tabular-nums" style={{ color: scoreColor }}>
                    {candidate.matchScore}
                  </span>
                )}
                {isApplied ? (
                  <span
                    className="absolute bottom-2 right-2 grid h-3.5 w-3.5 min-h-3.5 min-w-3.5 shrink-0 grow-0 basis-4 scale-y-95 place-items-center overflow-visible rounded-full bg-primary-foreground text-primary"
                    aria-hidden="true"
                  >
                    <Check className="size-2.5 shrink-0" strokeWidth={3} />
                  </span>
                ) : null}
              </div>
            </Button>
          </div>
        )
      })}
    </div>
  )
}
