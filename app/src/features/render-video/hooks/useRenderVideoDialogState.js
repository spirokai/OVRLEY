/**
 * Container hook for RenderVideoDialog.
 * Orchestrates derived state, synchronization effects, and event handlers.
 *
 * @param {object} props
 * @param {string} props.phase - Dialog phase ('closed'|'confirm'|'progress').
 * @param {object} props.settings - Current render settings draft; `renderTarget` selects the current video or a batch folder.
 * @param {function} props.onSettingsChange - Callback to update settings draft.
 * @param {function} props.onClose - Callback to close the dialog.
 * @param {function} props.onConfirm - Callback to start rendering the current video.
 * @returns {object} State and handlers for RenderVideoDialog.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { saveSinglePath } from '@/lib/file-dialog'
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
import { batchResultQueue, getImportedVideoExportRange, getRenderOutputExtension, getRenderSummaryItems } from '../utils/renderPresentation'
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
  const [showAllBatchVideos, setShowAllBatchVideos] = useState(false)
  const outputPath = settings?.outputPath
  const range = settings?.range
  const importedVideoRangePrefilledRef = useRef(false)
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
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    isBatchTarget,
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
    videoSyncOffsetSeconds,
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
    if (phase !== 'confirm') {
      importedVideoRangePrefilledRef.current = false
    }
  }, [phase])

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

  const handleApplyImportedVideoRange = useCallback(() => {
    if (!hasImportedVideo) {
      return
    }

    importedVideoRangePrefilledRef.current = true
    onSettingsChange({
      range: getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
    })
  }, [hasImportedVideo, importedVideoDuration, onSettingsChange, videoSyncOffsetSeconds])

  const handleRenderTargetChange = useCallback(
    (renderTarget) => {
      // Composite output needs a source video; the current-video target falls
      // back to transparent export when nothing is imported.
      onSettingsChange(renderTarget === 'current' && !hasImportedVideo ? { renderTarget, exportMode: 'transparent' } : { renderTarget })
    },
    [hasImportedVideo, onSettingsChange],
  )

  const handleExportModeChange = useCallback(
    (exportMode) => {
      if (exportMode !== 'transparent' || isBatchTarget || !hasImportedVideo || importedVideoRangePrefilledRef.current || range?.type === 'custom') {
        onSettingsChange({ exportMode })
        return
      }

      importedVideoRangePrefilledRef.current = true
      onSettingsChange({
        exportMode,
        range: getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
      })
    },
    [range, hasImportedVideo, importedVideoDuration, isBatchTarget, onSettingsChange, videoSyncOffsetSeconds],
  )

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
  const batchStartDisabled = renderStartDisabled || !batch.batchReady
  const handleConfirm = useCallback(async () => {
    if (isBatchTarget && !batch.batchReady) return
    try {
      await onConfirm(batch.request)
    } catch (error) {
      if (error.code === 'reinspectionRequired') batch.reject(error)
      else throw error
    }
  }, [batch, isBatchTarget, onConfirm])
  const handleCancel = useCallback(() => onCancel(settings.renderTarget), [onCancel, settings])

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
    // Composite output follows the source video's frame rate: the imported
    // video's when rendering it, each queued video's own in batch mode.
    fpsLocked: exportMode === 'composite' && (isBatchTarget || Boolean(lockedVideoFps)),
    fpsMode,
    handleAccelerationChange,
    handleApplyImportedVideoRange,
    handleCancel,
    isCancelling,
    handleCustomFpsChange,
    handleExportModeChange,
    handleFpsModeChange,
    handleRenderTargetChange,
    handleOutputFormatChange,
    handleQualityTypeChange,
    handleQualityValueChange,
    qualitySliderValue: settings?.qualityType === 'quality' ? invertQualityValue(settings.qualityValue) : settings?.qualityValue,
    hasImportedVideo,
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    isBatchTarget,
    isProgress,
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
    showBatchProgress: isBatchTarget && (batch.batchRunning || batchFinished),
    showContainerFps: !isBatchTarget || exportMode !== 'composite',
    showExportModeOverride: hasImportedVideo || isBatchTarget,
    showExportRangeSettings: exportMode !== 'composite' && !isBatchTarget,
    showVideoImportedBadge: hasImportedVideo && !isBatchTarget,
    showVideoRequiredBadge: !hasImportedVideo && !isBatchTarget,
    updateRateOptions,
  }
}
