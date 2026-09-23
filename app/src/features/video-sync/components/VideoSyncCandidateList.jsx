import { Check, RotateCwClock } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES } from '../data/videoSyncConstants'
import { buildVideoSyncCandidateCards } from '../utils/videoSyncPresentation'

/**
 * Renders calculated candidates and their calculation lifecycle states.
 *
 * @param {object} props Candidate state and callbacks.
 * @param {object} props.result Canonical candidate result for one scope.
 * @param {number} props.appliedOffset Current canonical applied offset.
 * @param {(scope: string, candidate: object) => void} props.onApply Applies one candidate.
 * @param {string} props.scope Candidate match scope.
 * @param {string} props.title Result section title.
 * @returns {JSX.Element|null} Candidate state or cards.
 */
export function VideoSyncCandidateList({ appliedOffset, result, onApply, scope, title }) {
  const { t } = useTranslation()
  const { candidates, error, hasSearched, status } = result

  if (!hasSearched && status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.IDLE) return null

  const disabled = status !== MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH
  return (
    <div className="space-y-4">
      {status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.ERROR ? (
        <p role="alert" className="rounded-sm bg-destructive/10 p-4 text-xs font-medium text-destructive">
          {error}
        </p>
      ) : null}
      {status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.STALE ? (
        <p className="text-xs font-medium text-amber-400">{t('videoSync.staleCandidates', 'Landmarks changed. Rerun the Sync again')}</p>
      ) : null}
      {candidates.length > 0 ? (
        <CandidateCards appliedOffset={appliedOffset} candidates={candidates} disabled={disabled} onApply={onApply} scope={scope} title={title} />
      ) : null}
      {hasSearched && status === MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES.FRESH && candidates.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t('videoSync.noCandidate', 'No candidate aligns at least two landmarks')}</p>
      ) : null}
    </div>
  )
}

function CandidateCards({ appliedOffset, candidates, disabled = false, onApply, scope, title }) {
  const { t } = useTranslation()
  const cards = buildVideoSyncCandidateCards(candidates, appliedOffset)

  return (
    <>
      <SectionHeading icon={RotateCwClock} title={title} variant="drawer" />
      <div className="space-y-1.5" role="list" aria-label={t('videoSync.candidates', 'Sync candidates')}>
        {cards.map(({ candidate, formattedOffset, isApplied, isLocationOnly, scoreColor }) => {
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
                  offset: formattedOffset,
                })}
                onClick={() => onApply(scope, candidate)}
              >
                <div className="flex min-w-0 flex-1 uppercase">
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5">
                      <span className={`text-base font-bold tabular-nums ${isApplied ? 'text-primary-foreground' : 'text-foreground'}`}>
                        {formattedOffset}
                      </span>
                    </div>
                    <div
                      className={`mt-0.5 text-[0.75rem] font-bold text-muted-foreground ${isApplied ? 'text-primary-foreground/60' : 'text-muted-foreground/50'}`}
                    >
                      {isLocationOnly ? (
                        <span>{t('videoSync.locationOnly', 'Location only')}</span>
                      ) : (
                        <span>
                          {t('videoSync.landmarksMatched', '{{matched}} / {{eligible}} landmarks matched', {
                            matched: candidate.matchedCount,
                            eligible: candidate.eligibleCount,
                          })}
                        </span>
                      )}
                    </div>
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
    </>
  )
}
