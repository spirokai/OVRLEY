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

import { useCallback, useEffect, useRef } from 'react'
import { cancelRender } from '@/api/backend'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { saveSinglePath } from '@/lib/file-dialog'
import { EXPORT_CODEC_LOOKUP, OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getExportCodecForSelection,
  getFirstAvailableAcceleration,
  getFirstAvailableMp4ExportCodec,
  getVisibleAccelerationOptions,
  isOutputFormatAvailable,
} from '../utils/codecUtils'
import { getRenderOutputExtension } from '../utils/render-output'
import useBatchRenderWorkflow from './useBatchRenderWorkflow'
import useRenderVideoDerivedState from './useRenderVideoDerivedState'

function getImportedVideoExportRange(durationSeconds, offsetSeconds) {
  return {
    type: 'custom',
    from: offsetSeconds,
    to: offsetSeconds + durationSeconds,
  }
}

export default function useRenderVideoDialogState({
  phase,
  settings,
  onSettingsChange,
  onClose,
  onConfirm,
  outputPathError,
  overwriteOpen,
  pendingOverwritePath,
  onOverwriteConfirm,
  onOverwriteCancel,
  submissionPending = false,
}) {
  const derived = useRenderVideoDerivedState({ settings })
  const batch = useBatchRenderWorkflow({ settings })
  const outputPath = settings?.outputPath
  const exportRange = settings?.exportRange
  const importedVideoRangePrefilledRef = useRef(false)
  const {
    availableCodecs,
    config,
    containerFps,
    defaultBitrateForCodec,
    exportMode,
    hasImportedVideo,
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    isBatchTarget,
    lockedVideoFps,
    platformOs,
    renderProgress,
    renderStartDisabled,
    renderingVideo,
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
        updateRate: normalizeUpdateRateForFps(fps, settings?.updateRate),
      })
    },
    updateRate: settings?.updateRate,
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
        exportCodec: 'prores_ks',
        exportAcceleration: 'cpu',
        exportBitrate: undefined,
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
          exportCodec: firstAvailableMp4Codec,
          exportAcceleration: EXPORT_CODEC_LOOKUP[firstAvailableMp4Codec]?.acceleration || 'cpu',
          exportBitrate: defaultBitrateForCodec(firstAvailableMp4Codec),
        })
      }
      return
    }

    if (!Number.isFinite(settings.exportBitrate)) {
      onSettingsChange({
        exportBitrate: defaultBitrateForCodec(settings.exportCodec),
      })
    }
  }, [availableCodecs, defaultBitrateForCodec, exportMode, onSettingsChange, platformOs, selectedCodecIsMp4, selectedExportCodecAvailable, settings])

  useEffect(() => {
    if (!settings) {
      return
    }

    const normalizedUpdateRate = normalizeUpdateRateForFps(updateRateFps, settings.updateRate)
    if (normalizedUpdateRate !== settings.updateRate) {
      onSettingsChange({ updateRate: normalizedUpdateRate })
    }
  }, [settings, updateRateFps, onSettingsChange])

  const handleCancel = useCallback(async () => {
    await cancelRender()
  }, [])

  const isProgress = phase === 'progress'

  const handleApplyImportedVideoRange = useCallback(() => {
    if (!hasImportedVideo) {
      return
    }

    importedVideoRangePrefilledRef.current = true
    onSettingsChange({
      exportRange: getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
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
      if (
        exportMode !== 'transparent' ||
        isBatchTarget ||
        !hasImportedVideo ||
        importedVideoRangePrefilledRef.current ||
        exportRange?.type === 'custom'
      ) {
        onSettingsChange({ exportMode })
        return
      }

      importedVideoRangePrefilledRef.current = true
      onSettingsChange({
        exportMode,
        exportRange: getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
      })
    },
    [exportRange, hasImportedVideo, importedVideoDuration, isBatchTarget, onSettingsChange, videoSyncOffsetSeconds],
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
      exportCodec: nextExportCodec,
      exportAcceleration: acceleration.value,
      exportBitrate: nextIsMp4Codec ? defaultBitrateForCodec(nextExportCodec) : undefined,
    })
  }

  const handleAccelerationChange = (value) => {
    const nextExportCodec = getExportCodecForSelection(selectedOutputFormatValue, value)
    if (!nextExportCodec) {
      return
    }

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: value,
      exportBitrate: selectedCodecIsMp4 ? defaultBitrateForCodec(nextExportCodec) : undefined,
    })
  }

  const batchStartDisabled =
    renderStartDisabled ||
    batch.batchRunning ||
    batch.batchQueue.length === 0 ||
    !batch.batchOutputFolder ||
    batch.batchQueue.some((item) => item.status === 'checking')

  return {
    ...batch,
    availableCodecs,
    batchStartDisabled,
    config,
    containerFps,
    exportMode,
    // Composite output follows the source video's frame rate: the imported
    // video's when rendering it, each queued video's own in batch mode.
    fpsLocked: exportMode === 'composite' && (isBatchTarget || Boolean(lockedVideoFps)),
    fpsMode,
    handleAccelerationChange,
    handleApplyImportedVideoRange,
    handleCancel,
    handleCustomFpsChange,
    handleExportModeChange,
    handleFpsModeChange,
    handleRenderTargetChange,
    handleOutputFormatChange,
    hasImportedVideo,
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    isBatchTarget,
    isProgress,
    isOutputFormatAvailable,
    lockedVideoFps,
    onClose,
    onConfirm,
    onOverwriteCancel,
    onOverwriteConfirm,
    onSettingsChange,
    OUTPUT_FORMATS,
    phase,
    platformOs,
    renderProgress,
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
    showContainerFps: !isBatchTarget || exportMode !== 'composite',
    showExportModeOverride: hasImportedVideo || isBatchTarget,
    showExportRangeSettings: exportMode !== 'composite' && !isBatchTarget,
    showVideoImportedBadge: hasImportedVideo && !isBatchTarget,
    showVideoRequiredBadge: !hasImportedVideo && !isBatchTarget,
    updateRateOptions,
  }
}
