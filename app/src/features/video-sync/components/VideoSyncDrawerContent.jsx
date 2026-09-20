import { Activity, CornerUpLeft, CornerUpRight, MapPin, OctagonMinus } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { SliderField } from '@/features/widget-editor/components/widgetFormControls'
import { VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH, VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES } from '../data/videoSyncConstants'
import { VideoSyncCandidateList } from './VideoSyncCandidateList'
import { VideoSyncControls } from './VideoSyncControls'
import { VideoSyncLandmarkList } from './VideoSyncLandmarkList'
import { getVideoSyncDetectionCounts } from '../utils/detectionSummary'

/**
 * Renders the dedicated manual video-sync drawer.
 *
 * @param {object} props Drawer state and actions from useVideoSyncWorkspace.
 * @returns {JSX.Element} Rendered manual video-sync drawer.
 */
export function VideoSyncDrawerContent({
  activeTab,
  appliedOffset,
  allResult,
  calculation,
  detection = null,
  landmarks,
  locationResult,
  onApplyAllCandidate,
  onApplyLocationCandidate,
  onCalculate,
  onCalculateLocation,
  onChangeLandmarkType,
  onClearLandmarks,
  onDeleteLandmark,
  onSpeedThresholdChange,
  onSpeedThresholdCommit,
  onTabChange,
  onTurnThresholdChange,
  onTurnThresholdCommit,
  speedThresholdDraftKmh,
  turnThresholdDraftDegrees,
  videoSummary,
  videoSync,
}) {
  const { t } = useTranslation()
  const isCalculating = calculation.isCalculating
  const canCalculate = calculation.eligibility.canCalculateAll
  const canCalculateLocation = calculation.eligibility.canCalculateLocation
  const speedValueLabel = `${speedThresholdDraftKmh.toFixed(1)}km/h`
  const turnValueLabel = `${turnThresholdDraftDegrees.toFixed(1)}°`
  const detectionCounts = getVideoSyncDetectionCounts(detection)

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-card">
      <Tabs value={activeTab} onValueChange={onTabChange} className="flex min-h-0 flex-1 flex-col">
        <div className="shrink-0">
          <TabsList variant="main" className="grid w-full grid-cols-2">
            <TabsTrigger variant="main" value="autodetect" className="text-sm cursor-pointer p-4 pt-2">
              {t('videoSync.autodetect', 'Autodetect')}
            </TabsTrigger>
            <TabsTrigger variant="main" value="manual" className="text-sm cursor-pointer p-4 pt-2">
              {t('videoSync.manualSync', 'Manual Sync')}
            </TabsTrigger>
          </TabsList>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto px-3 pb-4 thin-scrollbar">
          <TabsContent value="autodetect" className="outline-none pt-6">
            {videoSummary?.path ? <VideoSyncControls {...videoSync} importedVideoTimeSource={videoSummary.timeSource} /> : null}
          </TabsContent>

          <TabsContent value="manual" className="outline-none">
            <div className="space-y-8 pt-6">
              <VideoSyncLandmarkList
                landmarks={landmarks}
                onChangeType={onChangeLandmarkType}
                onClear={onClearLandmarks}
                onDelete={onDeleteLandmark}
              />

              <section className="space-y-4" aria-label={t('videoSync.detection', 'Detection sensitivity')}>
                <SectionHeading icon={Activity} title={t('videoSync.detection', 'Detection sensitivity')} variant="drawer" />

                <div className="space-y-4">
                  <div className="grid grid-cols-2 gap-4">
                    <SliderField
                      label={t('videoSync.speedSensitivity', 'Speed')}
                      value={speedThresholdDraftKmh}
                      min={VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH.min}
                      max={VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH.max}
                      step={0.5}
                      valueDisplay={speedValueLabel}
                      onSliderChange={onSpeedThresholdChange}
                      onSliderCommit={onSpeedThresholdCommit}
                    />

                    <SliderField
                      label={t('videoSync.turnSensitivity', 'Turning')}
                      value={turnThresholdDraftDegrees}
                      min={VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES.min}
                      max={VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES.max}
                      step={5}
                      valueDisplay={turnValueLabel}
                      onSliderChange={onTurnThresholdChange}
                      onSliderCommit={onTurnThresholdCommit}
                    />
                  </div>

                  <div
                    className="grid grid-cols-4 py-2 divide-x divide-border/50 text-[0.8rem] text-muted-foreground "
                    aria-label={t('videoSync.detectedEvents', 'Detected events')}
                    role="list"
                  >
                    <div
                      className="flex items-center justify-center gap-1 text-video-sync-turn"
                      role="listitem"
                      aria-label={`${detectionCounts.leftTurns} ${t('videoSync.leftTurns', 'Left Turns')}`}
                      title={t('videoSync.leftTurns', 'Left Turns')}
                    >
                      <span className="font-semibold tabular-nums pt-0.5">{detectionCounts.leftTurns}</span>
                      <CornerUpLeft className="size-4" strokeWidth={2.5} aria-hidden="true" />
                    </div>
                    <div
                      className="flex items-center justify-center gap-1 text-video-sync-stop"
                      role="listitem"
                      aria-label={`${detectionCounts.stops} ${t('videoSync.stops', 'Stops')}`}
                      title={t('videoSync.stops', 'Stops')}
                    >
                      <span className="font-semibold tabular-nums pt-0.5">{detectionCounts.stops}</span>
                      <OctagonMinus className="size-4" strokeWidth={2.5} aria-hidden="true" />
                    </div>
                    <div
                      className="flex items-center justify-center gap-1 text-video-sync-turn"
                      role="listitem"
                      aria-label={`${detectionCounts.rightTurns} ${t('videoSync.rightTurns', 'Right Turns')}`}
                      title={t('videoSync.rightTurns', 'Right Turns')}
                    >
                      <span className="font-semibold tabular-nums pt-0.5">{detectionCounts.rightTurns}</span>
                      <CornerUpRight className="size-4" strokeWidth={2.5} aria-hidden="true" />
                    </div>
                    <div
                      className="flex items-center justify-center gap-1 text-video-sync-location"
                      role="listitem"
                      aria-label={`${detectionCounts.locations} ${t('videoSync.locations', 'Locations')}`}
                      title={t('videoSync.locations', 'Locations')}
                    >
                      <span className="font-semibold tabular-nums pt-0.5">{detectionCounts.locations}</span>
                      <MapPin className="size-3.5" strokeWidth={2.5} aria-hidden="true" />
                    </div>
                  </div>

                  <div className="space-y-2">
                    <Button type="button" className="w-full h-9" disabled={!canCalculate || isCalculating} onClick={onCalculate}>
                      {t('videoSync.syncLandmarks', 'Sync All Landmarks')}
                    </Button>
                    <Button type="button" className="w-full h-9" disabled={!canCalculateLocation || isCalculating} onClick={onCalculateLocation}>
                      {t('videoSync.syncLocation', 'Match Locations Only')}
                    </Button>
                  </div>
                  {!canCalculate && calculation.eligibility.allExplanation ? (
                    <p className="text-[10px] text-muted-foreground pt-2 pb-2">{calculation.eligibility.allExplanation}</p>
                  ) : null}

                  <VideoSyncCandidateList
                    appliedOffset={appliedOffset}
                    candidates={locationResult.candidates}
                    error={locationResult.error}
                    hasSearched={locationResult.hasSearched}
                    onApply={onApplyLocationCandidate}
                    status={locationResult.status}
                    title={t('videoSync.locationSyncCandidates', 'LOCATION SYNC CANDIDATE')}
                  />

                  <VideoSyncCandidateList
                    appliedOffset={appliedOffset}
                    candidates={allResult.candidates}
                    error={allResult.error}
                    hasSearched={allResult.hasSearched}
                    onApply={onApplyAllCandidate}
                    status={allResult.status}
                    title={t('videoSync.syncCandidates', 'SYNC CANDIDATES')}
                  />
                </div>
              </section>
            </div>
          </TabsContent>
        </div>
      </Tabs>
    </div>
  )
}
