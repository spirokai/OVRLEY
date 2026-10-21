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
import { useRenderDialogInputs, useRenderRangeActivity } from '@/hooks/useAppStoreSelectors'
import { getActivityDurationSeconds, getCustomExportRangeDefault, setExportRangeBoundary } from '@/features/overlay-editor/utils/exportRange'
import { createBatchItemProgress, createBatchProgress, isBatchFinished } from '../utils/renderPresentation'

/**
 * @param {object} options Editable range, render frame rate and commit callback; the boundaries snap to the render frames.
 * @returns {object} Shared range control presentation and input handlers.
 */
export function useExportRangeSettings({ range, fps, onExportRangeChange }) {
  const activity = useRenderRangeActivity()
  return {
    isCustom: range.type === 'custom',
    fromSecond: range.from,
    toSecond: range.to,
    rangeFps: { num: fps, den: 1 },
    handleCustomChange: (checked) =>
      onExportRangeChange(checked ? getCustomExportRangeDefault(range, getActivityDurationSeconds(activity)) : { ...range, type: 'all' }),
    handleFromCommit: (second) => onExportRangeChange(setExportRangeBoundary(range, 'from', second)),
    handleToCommit: (second) => onExportRangeChange(setExportRangeBoundary(range, 'to', second)),
  }
}

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
  } = useRenderDialogInputs()
  // A closed dialog has no draft; its hidden controls read committed settings.
  const displaySettings = settings === null ? renderSettings : settings
  const hasImportedVideo = importedVideos.length > 0
  const videoFormat = useMemo(() => deriveCompatibility(importedVideos).reference, [importedVideos])
  const videoBounds = useMemo(() => getVideosTimelineBounds(joinVideoEdits(importedVideos, videoEdits)), [importedVideos, videoEdits])
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
    updateRateOptions,
    videoBounds,
  }
}
