/**
 * Container hook for RenderVideoDialog.
 * Orchestrates derived state, synchronization effects, and event handlers.
 *
 * @param {object} props
 * @param {string} props.phase - Dialog phase ('closed'|'confirm'|'progress').
 * @param {object} props.settings - Current render settings draft; with videos `outputMode` selects one combined file or one file per video.
 * @param {function} props.onSettingsChange - Callback to update settings draft.
 * @param {function} props.onClose - Callback to close the dialog.
 * @param {function} props.onConfirm - Callback to start rendering the current video.
 * @returns {object} State and handlers for RenderVideoDialog.
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { saveSinglePath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'
import { OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getExportCodecForSelection,
  getFirstAvailableAcceleration,
  getFirstAvailableMp4ExportCodec,
  getVisibleAccelerationOptions,
  isOutputFormatAvailable,
  getDefaultQuality,
  invertQualityValue,
} from '../utils/codecUtils'
import { batchResultQueue, getRenderOutputExtension, getRenderSummaryItems } from '../utils/renderPresentation'
import { useTranslation } from 'react-i18next'
import useRenderVideoDerivedState from './useRenderVideoDerivedState'
import useBatchInspection from './useBatchInspection'

export default function useRenderVideoDialogState({
  phase,
  settings,
  onSettingsChange,
  onClose,
  onConfirm,
  onCancel,
  outputPathError,
  overwriteOpen,
  pendingOverwritePath,
  onOverwriteConfirm,
  onOverwriteCancel,
}) {
  const { t } = useTranslation()
  const derived = useRenderVideoDerivedState({ settings })
  const batch = useBatchInspection({ phase, settings })
  const [showAllBatchVideos, setShowAllBatchVideos] = useState(true)
  const outputPath = settings?.outputPath
  // Only an external activity has a timeline of its own to render whole; embedded telemetry belongs to its clip.
  const hasExternalActivity = useStore((state) => state.parsedActivity !== null)
  const {
    availableCodecs,
    batchSnapshot,
    batchFinished,
    batchActiveItemId,
    batchProgress,
    currentItemProgress,
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
    submissionPending,
    resolutionMismatch,
    selectedAccelerationOptions,
    selectedAccelerationValue,
    selectedCodecIsMp4,
    selectedExportCodecAvailable,
    selectedOutputFormatValue,
    updateRateFps,
    updateRateOptions,
    videoBounds,
  } = derived

  const { fpsMode, handleFpsModeChange, handleCustomFpsChange } = useFpsMode({
    fps: settings?.fps,
    onFpsChange: (fps) => {
      onSettingsChange({
        fps,
        widgetUpdateRate: normalizeUpdateRateForFps(fps, settings?.widgetUpdateRate),
      })
    },
    updateRate: settings?.widgetUpdateRate,
  })

  useEffect(() => {
    if (!settings) {
      return
    }

    // Codec selection follows the active export pipeline: transparent exports
    // cannot keep MP4 codecs, while composite exports must land on one.
    if (exportMode !== 'composite' && selectedCodecIsMp4) {
      onSettingsChange({
        codec: 'prores_ks',
      })
      return
    }

    if (exportMode !== 'composite') {
      return
    }

    const firstAvailableMp4Codec = getFirstAvailableMp4ExportCodec(platformOs, availableCodecs)

    if (!selectedCodecIsMp4 || !selectedExportCodecAvailable) {
      if (firstAvailableMp4Codec) {
        onSettingsChange({
          codec: firstAvailableMp4Codec,
          qualityValue:
            settings.qualityType === 'quality' ? getDefaultQuality(firstAvailableMp4Codec) : defaultBitrateForCodec(firstAvailableMp4Codec),
        })
      }
      return
    }
  }, [availableCodecs, defaultBitrateForCodec, exportMode, onSettingsChange, platformOs, selectedCodecIsMp4, selectedExportCodecAvailable, settings])

  useEffect(() => {
    if (!settings) {
      return
    }

    const normalizedUpdateRate = normalizeUpdateRateForFps(updateRateFps, settings.widgetUpdateRate)
    if (normalizedUpdateRate !== settings.widgetUpdateRate) {
      onSettingsChange({ widgetUpdateRate: normalizedUpdateRate })
    }
  }, [settings, updateRateFps, onSettingsChange])

  const isProgress = phase === 'progress'

  // The whole activity renders as one transparent file, so composite export and separate files turn it off.
  const handleOutputModeChange = useCallback(
    (outputMode) => onSettingsChange(outputMode === 'separate' ? { outputMode, renderFullActivity: false } : { outputMode }),
    [onSettingsChange],
  )
  const handleExportModeChange = useCallback(
    (exportMode) => onSettingsChange(exportMode === 'composite' ? { exportMode, renderFullActivity: false } : { exportMode }),
    [onSettingsChange],
  )
  const handleRenderFullActivityChange = useCallback((renderFullActivity) => onSettingsChange({ renderFullActivity }), [onSettingsChange])
  // With videos an export follows their parts; rendering the whole activity overrides them with one overlay render.
  const renderFullActivity = hasImportedVideo && settings?.renderFullActivity === true
  const rendersVideoParts = hasImportedVideo && !renderFullActivity

  const handleOutputPathCommit = useCallback(
    (nextOutputPath = outputPath) => onSettingsChange({ outputPath: nextOutputPath }),
    [onSettingsChange, outputPath],
  )

  const handleBrowse = useCallback(async () => {
    if (!outputPath) {
      return
    }
    const selectedPath = await saveSinglePath(outputPath, getRenderOutputExtension(exportMode))
    if (selectedPath) {
      onSettingsChange({ outputPath: selectedPath })
    }
  }, [exportMode, onSettingsChange, outputPath])

  const handleOutputFormatChange = (value) => {
    const format = OUTPUT_FORMATS_BY_VALUE[value]
    if (!format) {
      return
    }

    const acceleration =
      getVisibleAccelerationOptions(format, platformOs, availableCodecs).find(
        (option) => option.value === selectedAccelerationValue && option.available,
      ) || getFirstAvailableAcceleration(format, platformOs, availableCodecs)

    if (!acceleration) {
      return
    }

    const nextExportCodec = getExportCodecForSelection(format.value, acceleration.value)
    const nextIsMp4Codec = format.group === 'mp4'

    onSettingsChange({
      codec: nextExportCodec,
      ...(nextIsMp4Codec && {
        qualityValue: settings.qualityType === 'quality' ? getDefaultQuality(nextExportCodec) : defaultBitrateForCodec(nextExportCodec),
      }),
    })
  }

  const handleAccelerationChange = (value) => {
    const nextExportCodec = getExportCodecForSelection(selectedOutputFormatValue, value)
    if (!nextExportCodec) {
      return
    }

    onSettingsChange({
      codec: nextExportCodec,
    })
  }

  const handleQualityTypeChange = (qualityType) => {
    onSettingsChange({
      qualityType,
      qualityValue: qualityType === 'quality' ? getDefaultQuality(settings.codec) : defaultBitrateForCodec(settings.codec),
    })
  }

  const handleQualityValueChange = ([value]) => {
    onSettingsChange({ qualityValue: settings.qualityType === 'quality' ? invertQualityValue(value) : value })
  }

  const batchItems = batchSnapshot?.items ?? null
  const batchQueue = useMemo(() => (batchItems === null ? batch.batchQueue : batchResultQueue(batchItems)), [batchItems, batch.batchQueue])
  const visibleBatchQueue = useMemo(
    () => (showAllBatchVideos ? batchQueue : batchQueue.filter((item) => item.status !== 'blocked')),
    [batchQueue, showAllBatchVideos],
  )
  const batchStartDisabled = rendersVideoParts
    ? renderStartDisabled || !batch.batchReady
    : renderStartDisabled || submissionPending || !settings?.outputPath
  // Video parts render through the batch service; the activity alone (without videos, or whole) through a single render.
  const handleConfirm = useCallback(async () => {
    if (rendersVideoParts && !batch.batchReady) return
    try {
      await onConfirm(rendersVideoParts ? batch.request : null)
    } catch (error) {
      if (error.code === 'reinspectionRequired') batch.reject(error)
      else throw error
    }
  }, [batch, rendersVideoParts, onConfirm])
  const handleCancel = useCallback(() => onCancel(rendersVideoParts ? 'batch' : 'single'), [rendersVideoParts, onCancel])

  return {
    ...batch,
    availableCodecs,
    batchSnapshot,
    batchFinished,
    batchActiveItemId,
    batchProgress,
    batchQueue,
    batchStartDisabled,
    config,
    containerFps,
    currentItemProgress,
    exportMode,
    // Composite output follows the source videos' frame rate: their shared one in
    // a combined file, each video's own in separate files.
    fpsLocked: exportMode === 'composite' && (isSeparate || Boolean(lockedVideoFps)),
    fpsMode,
    handleAccelerationChange,
    handleRenderFullActivityChange,
    renderFullActivity,
    handleCancel,
    isCancelling,
    handleCustomFpsChange,
    handleExportModeChange,
    handleFpsModeChange,
    handleOutputModeChange,
    handleOutputFormatChange,
    handleQualityTypeChange,
    handleQualityValueChange,
    qualitySliderValue: settings?.qualityType === 'quality' ? invertQualityValue(settings.qualityValue) : settings?.qualityValue,
    hasImportedVideo,
    hasMultipleVideos,
    importedVideoFps,
    importedVideoResolution,
    isSeparate,
    isProgress,
    videoBounds,
    isOutputFormatAvailable,
    lockedVideoFps,
    onClose,
    onConfirm: handleConfirm,
    onOverwriteCancel,
    onOverwriteConfirm,
    onSettingsChange,
    OUTPUT_FORMATS,
    phase,
    platformOs,
    renderProgress,
    renderSummaryItems: settings ? getRenderSummaryItems({ ...derived, settings, OUTPUT_FORMATS }, t) : [],
    renderStartDisabled: renderStartDisabled || submissionPending || !settings?.outputPath,
    renderingVideo,
    resolutionMismatch,
    selectedAccelerationOptions,
    selectedAccelerationValue,
    selectedCodecIsMp4,
    selectedOutputFormatValue,
    settings,
    handleBrowse,
    handleOutputPathCommit,
    outputPathError,
    overwriteOpen,
    pendingOverwritePath,
    submissionPending,
    settingsLocked: batch.batchRunning,
    showAllBatchVideos,
    setShowAllBatchVideos,
    visibleBatchQueue,
    showBatchProgress: hasImportedVideo && (batch.batchRunning || batchFinished),
    showContainerFps: !isSeparate || exportMode !== 'composite',
    showExportModeOverride: hasImportedVideo,
    showRenderFullActivity: hasImportedVideo && hasExternalActivity && exportMode === 'transparent' && !isSeparate,
    showVideoImportedBadge: hasImportedVideo && !isSeparate,
    showVideoRequiredBadge: !hasImportedVideo,
    updateRateOptions,
  }
}
