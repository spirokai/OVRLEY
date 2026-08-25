/**
 * Container hook for RenderVideoDialog.
 * Orchestrates derived state, synchronization effects, and event handlers.
 *
 * @param {object} props
 * @param {string} props.phase - Dialog phase ('closed'|'confirm'|'progress').
 * @param {object} props.settings - Current render settings draft.
 * @param {function} props.onSettingsChange - Callback to update settings draft.
 * @param {function} props.onClose - Callback to close the dialog.
 * @param {function} props.onConfirm - Callback to start rendering.
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
import { getRenderOutputExtension, normalizeRenderOutputPath } from '../utils/render-output'
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
  onOutputPathChange,
  outputPathError,
  overwriteOpen,
  pendingOverwritePath,
  onOverwriteConfirm,
  onOverwriteCancel,
  submissionPending = false,
}) {
  const derived = useRenderVideoDerivedState({ settings })
  const outputPath = settings?.outputPath
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
      exportRange: {
        ...(settings?.exportRange || {}),
        ...getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
      },
    })
  }, [hasImportedVideo, importedVideoDuration, onSettingsChange, settings?.exportRange, videoSyncOffsetSeconds])

  const handleExportModeChange = useCallback(
    (exportMode) => {
      const normalizedOutputPath = outputPath ? normalizeRenderOutputPath(outputPath, exportMode) : undefined
      // Only the first switch into transparent mode auto-prefills the imported
      // video span; after that, manual edits stay intact until the dialog closes.
      if (exportMode === 'transparent' && hasImportedVideo && !importedVideoRangePrefilledRef.current) {
        importedVideoRangePrefilledRef.current = true
        onSettingsChange({
          exportMode,
          outputPath: normalizedOutputPath,
          exportRange: {
            ...(settings?.exportRange || {}),
            ...getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
          },
        })
        return
      }

      onSettingsChange({ exportMode, ...(normalizedOutputPath ? { outputPath: normalizedOutputPath } : {}) })
    },
    [hasImportedVideo, importedVideoDuration, onSettingsChange, outputPath, settings?.exportRange, videoSyncOffsetSeconds],
  )

  const handleOutputPathCommit = useCallback(
    (nextOutputPath = outputPath) => {
      if (!nextOutputPath) {
        return
      }
      const normalizedPath = normalizeRenderOutputPath(nextOutputPath, exportMode)
      onOutputPathChange ? onOutputPathChange(normalizedPath) : onSettingsChange({ outputPath: normalizedPath })
    },
    [exportMode, onOutputPathChange, onSettingsChange, outputPath],
  )

  const handleBrowse = useCallback(async () => {
    if (!outputPath) {
      return
    }
    const selectedPath = await saveSinglePath(outputPath, getRenderOutputExtension(exportMode))
    if (selectedPath) {
      const normalizedPath = normalizeRenderOutputPath(selectedPath, exportMode)
      onOutputPathChange ? onOutputPathChange(normalizedPath) : onSettingsChange({ outputPath: normalizedPath })
    }
  }, [exportMode, onOutputPathChange, onSettingsChange, outputPath])

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

  const dialogTitle = 'Export Settings'

  return {
    availableCodecs,
    config,
    containerFps,
    dialogTitle,
    exportMode,
    fpsMode,
    handleAccelerationChange,
    handleApplyImportedVideoRange,
    handleCancel,
    handleCustomFpsChange,
    handleExportModeChange,
    handleFpsModeChange,
    handleOutputFormatChange,
    hasImportedVideo,
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    isProgress,
    isOutputFormatAvailable,
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
    showExportModeOverride: hasImportedVideo,
    showExportRangeSettings: exportMode !== 'composite',
    updateRateOptions,
  }
}
