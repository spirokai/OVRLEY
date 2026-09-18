import { Activity, Bell, ChevronDown, ChevronUp, Clock3, CornerUpLeft, CornerUpRight, OctagonMinus, RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { BlurInput } from '@/components/ui/blur-input'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { SliderField } from '@/features/widget-editor/components/widgetFormControls'
import { VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH, VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES } from '../data/videoSyncConstants'
import { VideoSyncCandidateList } from './VideoSyncCandidateList'
import { VideoSyncLandmarkList } from './VideoSyncLandmarkList'
import { getVideoSyncDetectionCounts } from '../utils/detectionSummary'

function VideoSyncControls({
  activitySummary,
  canResetCreationTime,
  filenameCreationTimeAvailable,
  importedVideoTimeSource,
  offsetInput,
  timezone,
  videoSyncTimezoneMode,
  videoSyncWarning,
  computeVideoSync,
  incrementOffset,
  resetVideoCreationTime,
  setOffsetInput,
  setVideoCreationTimeFromFilename,
  setVideoSyncTimezoneMode,
  submitOffsetInput,
}) {
  const { t } = useTranslation()
  if (!activitySummary) return null

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <SectionHeading icon={Clock3} title={t('toolbar.videoSync', 'Video Sync')} variant="drawer" />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="ml-2 h-6 w-6 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
          disabled={!canResetCreationTime}
          onClick={resetVideoCreationTime}
          aria-label={t('toolbar.restoreDetectedVideoCreationTime', 'Restore detected video creation time')}
        >
          <RotateCcw className="h-3 w-3" />
        </Button>
      </div>

      <div className="space-y-3">
        <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('toolbar.syncOffset', 'Sync Offset')}</Label>
        <div className="grid grid-cols-2 items-center gap-4">
          <div className="relative flex-1">
            <BlurInput
              type="text"
              value={offsetInput}
              onChange={(event) => setOffsetInput(event.target.value)}
              onBlur={(event) => submitOffsetInput(event.target.value)}
              className="h-9 text-xs pr-11 w-full border border-border/70"
              placeholder={t('toolbar.secondsOrMmss', 'Seconds or MM:SS')}
            />
            <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
              <button
                type="button"
                aria-label={t('toolbar.increaseSyncOffset', 'Increase sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => incrementOffset(0.1)}
              >
                <ChevronUp className="h-3 w-3" />
              </button>
              <div className="h-px bg-border/60" />
              <button
                type="button"
                aria-label={t('toolbar.decreaseSyncOffset', 'Decrease sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => incrementOffset(-0.1)}
              >
                <ChevronDown className="h-3 w-3" />
              </button>
            </div>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 border-border/80 bg-surface-elevated px-3 text-xs font-semibold text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
            disabled={!activitySummary}
            onClick={() => computeVideoSync(activitySummary)}
            aria-keyshortcuts="Mod+Shift+A"
          >
            {t('toolbar.autosync', 'Auto-sync')}
          </Button>
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-1">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('toolbar.creationTime', 'Creation Time')}</Label>
          <Tabs
            value={canResetCreationTime ? 'filename' : 'detected'}
            onValueChange={(value) => (value === 'filename' ? setVideoCreationTimeFromFilename() : resetVideoCreationTime())}
          >
            <TabsList variant="toolbar" className="grid h-8 w-full grid-cols-2 p-0.5">
              <TabsTrigger variant="toolbar" value="detected" className="h-full px-2 text-[0.7rem]">
                {t('toolbar.detected', 'Detected')}
              </TabsTrigger>
              <TabsTrigger variant="toolbar" value="filename" className="h-full px-2 text-[0.7rem]" disabled={!filenameCreationTimeAvailable}>
                {t('toolbar.filename', 'Filename')}
              </TabsTrigger>
            </TabsList>
          </Tabs>
          {timezone ? (
            <>
              <Label htmlFor="video-sync-timezone-toggle" className="mb-2 text-[10px] text-muted-foreground uppercase font-bold">
                {t('toolbar.applyTimezone', 'Apply Timezone')}
              </Label>
              <div className="mb-2 flex items-center gap-2">
                <Switch
                  id="video-sync-timezone-toggle"
                  checked={videoSyncTimezoneMode === 'utc'}
                  disabled={importedVideoTimeSource === 'filename'}
                  onCheckedChange={(checked) => setVideoSyncTimezoneMode(checked ? 'utc' : 'local')}
                  aria-label={t('toolbar.applyTimezone', 'Apply Timezone')}
                />
              </div>
            </>
          ) : null}
        </div>
      </div>
      {videoSyncWarning ? (
        <div className="flex gap-2 items-center rounded-sm bg-amber-500/15 p-2 pl-4 text-amber-400">
          <Bell className="h-3 w-3 shrink-0" />
          <p className="text-[0.65rem] font-semibold leading-tight">{videoSyncWarning}</p>
        </div>
      ) : null}
    </section>
  )
}

/**
 * Renders the dedicated manual video-sync drawer.
 *
 * @param {object} props Drawer state and actions from useVideoSyncWorkspace.
 * @returns {JSX.Element} Rendered manual video-sync drawer.
 */
export function VideoSyncDrawerContent({
  appliedOffset,
  candidates,
  candidateStatus,
  calculation,
  detection = null,
  error,
  hasSearched,
  landmarks,
  onApplyCandidate,
  onCalculate,
  onChangeLandmarkType,
  onClearLandmarks,
  onDeleteLandmark,
  onSpeedThresholdChange,
  onSpeedThresholdCommit,
  onTurnThresholdChange,
  onTurnThresholdCommit,
  speedThresholdDraftKmh,
  turnThresholdDraftDegrees,
  videoSummary,
  videoSync,
}) {
  const { t } = useTranslation()
  const isCalculating = calculation.isCalculating
  const canCalculate = calculation.eligibility.canCalculate
  const speedValueLabel = `${speedThresholdDraftKmh.toFixed(1)}km/h`
  const turnValueLabel = `${turnThresholdDraftDegrees.toFixed(1)}°`
  const detectionCounts = getVideoSyncDetectionCounts(detection)

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      {videoSummary?.path ? (
        <div className="border-b border-border/80 pb-6">
          <VideoSyncControls {...videoSync} importedVideoTimeSource={videoSummary.timeSource} />
        </div>
      ) : null}

      <div className="space-y-8 pt-6">
        <VideoSyncLandmarkList landmarks={landmarks} onChangeType={onChangeLandmarkType} onClear={onClearLandmarks} onDelete={onDeleteLandmark} />

        <section className="space-y-4" aria-label={t('videoSync.detection', 'Detection senstivity')}>
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
              className="grid grid-cols-3 py-2 divide-x divide-border/50 text-[0.8rem] text-muted-foreground"
              aria-label={t('videoSync.detectedEvents', 'Detected events')}
              role="list"
            >
              <div
                className="flex items-center justify-center gap-1 text-video-sync-turn"
                role="listitem"
                aria-label={`${detectionCounts.leftTurns} ${t('videoSync.leftTurns', 'Left Turns')}`}
                title={t('videoSync.leftTurns', 'Left Turns')}
              >
                <span className="font-semibold tabular-nums">{detectionCounts.leftTurns}</span>
                <CornerUpLeft className="size-4" strokeWidth={2.5} aria-hidden="true" />
              </div>
              <div
                className="flex items-center justify-center gap-1 text-video-sync-stop"
                role="listitem"
                aria-label={`${detectionCounts.stops} ${t('videoSync.stops', 'Stops')}`}
                title={t('videoSync.stops', 'Stops')}
              >
                <span className="font-semibold tabular-nums">{detectionCounts.stops}</span>
                <OctagonMinus className="size-4" strokeWidth={2.5} aria-hidden="true" />
              </div>
              <div
                className="flex items-center justify-center gap-1 text-video-sync-turn"
                role="listitem"
                aria-label={`${detectionCounts.rightTurns} ${t('videoSync.rightTurns', 'Right Turns')}`}
                title={t('videoSync.rightTurns', 'Right Turns')}
              >
                <span className="font-semibold tabular-nums">{detectionCounts.rightTurns}</span>
                <CornerUpRight className="size-4" strokeWidth={2.5} aria-hidden="true" />
              </div>
            </div>

            <Button
              type="button"
              className="grid h-9 w-full gap-2 text-xs font-semibold"
              disabled={!canCalculate || isCalculating}
              onClick={onCalculate}
            >
              {t('videoSync.syncLandmarks', 'Sync Landmarks')}
            </Button>
            {!canCalculate && calculation.eligibility.explanation ? (
              <p className="text-[10px] text-muted-foreground">{calculation.eligibility.explanation}</p>
            ) : null}
            <VideoSyncCandidateList
              appliedOffset={appliedOffset}
              candidates={candidates}
              error={error}
              hasSearched={hasSearched}
              onApply={onApplyCandidate}
              status={candidateStatus}
            />
          </div>
        </section>
      </div>
    </div>
  )
}
