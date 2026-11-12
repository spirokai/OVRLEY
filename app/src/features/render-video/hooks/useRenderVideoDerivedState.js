/**
 * Reads render store state and computes all derived values needed by
 * RenderVideoDialog. Single concern: data access + transformation.
 * Composed by useRenderVideoDialogState.
 *
 * @param {object} params
 * @param {object|null} params.settings - Current render settings draft.
 * @returns {object} Store values and derived render dialog state.
 */

import { useCallback, useMemo } from 'react'
import { deriveCompatibility, getVideosTimelineBounds, joinVideoEdits } from '@/lib/video-program/video-collection'
import { OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getAccelerationValueForSettings,
  getOutputFormatForExportCodec,
  getVisibleAccelerationOptions,
  isMp4Codec,
  resolutionsMismatch,
} from '../utils/codecUtils'
import { getContainerFps, getUpdateRateOptions } from '@/lib/update-rate'
import { getDefaultBitrate } from '../data/bitrateDefaults'
import { useRenderDialogInputs } from '@/hooks/useAppStoreSelectors'
import { getActivityDurationSeconds } from '@/lib/activity/activity-summary'
import { resolveActivityWindow } from '@/lib/activity/activity-trim'
import { createBatchItemProgress, createBatchProgress, isBatchFinished } from '../utils/renderPresentation'

export default function useRenderVideoDerivedState({ settings }) {
  const {
    renderingVideo,
    rendererBusy,
    platformOs,
    availableCodecs,
    config,
    importedVideos,
    videoEdits,
    renderProgress,
    batchSnapshot,
    renderSettings,
    renderSubmissionTarget,
    isCancelling,
    parsedActivity,
    activityTrim,
  } = useRenderDialogInputs()
  // A closed dialog has no draft; its hidden controls read committed settings.
  const displaySettings = settings === null ? renderSettings : settings
  const hasImportedVideo = importedVideos.length > 0
  // Output modes and the video queue only matter when there is more than one video.
  const hasMultipleVideos = importedVideos.length > 1
  const videoFormat = useMemo(() => deriveCompatibility(importedVideos).reference, [importedVideos])
  const videoBounds = useMemo(() => getVideosTimelineBounds(joinVideoEdits(importedVideos, videoEdits)), [importedVideos, videoEdits])
  // The trim decides the overlay only without videos; the full activity switch ignores it.
  const activityDurationSeconds = getActivityDurationSeconds(parsedActivity)
  const trimmedActivityWindow = useMemo(
    () => resolveActivityWindow(activityDurationSeconds, hasImportedVideo ? null : activityTrim),
    [activityDurationSeconds, activityTrim, hasImportedVideo],
  )
  const importedVideoFps = videoFormat === null ? null : videoFormat.fps.num / videoFormat.fps.den
  const importedVideoResolution = videoFormat?.resolution ?? null
  const isSeparate = displaySettings.outputMode === 'separate'
  // Separate files keep each video's FPS; a combined file has the videos' shared FPS.
  const lockedVideoFps = isSeparate || !hasImportedVideo ? null : importedVideoFps
  const exportMode = displaySettings.exportMode
  const updateRateFps = useMemo(
    () => (exportMode === 'composite' && lockedVideoFps !== null ? Math.round(lockedVideoFps) : displaySettings.fps),
    [exportMode, lockedVideoFps, displaySettings.fps],
  )
  const updateRateOptions = useMemo(() => getUpdateRateOptions(updateRateFps), [updateRateFps])
  const containerFps = useMemo(
    () => getContainerFps(updateRateFps, displaySettings.widgetUpdateRate),
    [updateRateFps, displaySettings.widgetUpdateRate],
  )
  const selectedOutputFormat = getOutputFormatForExportCodec(displaySettings.codec)
  const selectedOutputFormatValue = selectedOutputFormat.value
  const selectedAccelerationValue = getAccelerationValueForSettings(displaySettings)
  const selectedAccelerationOptions = useMemo(
    () => getVisibleAccelerationOptions(OUTPUT_FORMATS_BY_VALUE[selectedOutputFormatValue], platformOs, availableCodecs),
    [availableCodecs, platformOs, selectedOutputFormatValue],
  )
  const selectedCodecIsMp4 = isMp4Codec(displaySettings.codec)
  const selectedExportCodecAvailable = selectedAccelerationOptions.some((option) => option.value === selectedAccelerationValue && option.available)
  const resolutionMismatch = !isSeparate && resolutionsMismatch(config?.scene, importedVideoResolution)
  const renderStartDisabled =
    rendererBusy ||
    resolutionMismatch ||
    (exportMode === 'composite' && (!selectedCodecIsMp4 || !selectedExportCodecAvailable)) ||
    (exportMode !== 'composite' && selectedCodecIsMp4)

  const defaultBitrateForCodec = useCallback(
    (codec) =>
      getDefaultBitrate(
        hasImportedVideo ? importedVideoResolution.width : config.scene.width,
        hasImportedVideo ? importedVideoResolution.height : config.scene.height,
        hasImportedVideo ? importedVideoFps : displaySettings.fps,
        codec,
      ),
    [config, displaySettings.fps, hasImportedVideo, importedVideoFps, importedVideoResolution],
  )

  return {
    activityDurationSeconds,
    availableCodecs,
    batchSnapshot,
    batchFinished: isBatchFinished(batchSnapshot),
    batchActiveItemId: batchSnapshot?.activeItemId ?? null,
    batchProgress: createBatchProgress(batchSnapshot),
    currentItemProgress: createBatchItemProgress(batchSnapshot),
    config,
    containerFps,
    defaultBitrateForCodec,
    exportMode,
    hasImportedVideo,
    hasMultipleVideos,
    importedVideoFps,
    importedVideoResolution,
    isSeparate,
    isCancelling,
    lockedVideoFps,
    platformOs,
    renderProgress,
    renderStartDisabled,
    renderingVideo,
    submissionPending: renderSubmissionTarget === 'single',
    resolutionMismatch,
    selectedAccelerationOptions,
    selectedAccelerationValue,
    selectedCodecIsMp4,
    selectedExportCodecAvailable,
    selectedOutputFormatValue,
    updateRateFps,
    trimmedActivityWindow,
    updateRateOptions,
    videoBounds,
  }
}
