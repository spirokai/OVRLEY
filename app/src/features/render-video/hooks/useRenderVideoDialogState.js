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
import { formatExportRangeTime } from '@/features/overlay-editor/utils/exportRange'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { useFpsMode } from '@/hooks/useFpsMode'
import { EXPORT_CODEC_LOOKUP, OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getExportCodecForSelection,
  getFirstAvailableAcceleration,
  getFirstAvailableMp4ExportCodec,
  getVisibleAccelerationOptions,
  isOutputFormatAvailable,
} from '../utils/codecUtils'
import useRenderVideoDerivedState from './useRenderVideoDerivedState'

function getImportedVideoExportRange(durationSeconds, offsetSeconds) {
  const start = Math.max(0, Number(offsetSeconds) || 0)
  const duration = Math.max(0, Number(durationSeconds) || 0)

  return {
    type: 'custom',
    fromTime: formatExportRangeTime(start),
    toTime: formatExportRangeTime(start + duration),
  }
}

export default function useRenderVideoDialogState({ phase, settings, onSettingsChange, onClose, onConfirm }) {
  const derived = useRenderVideoDerivedState({ settings })
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

  const handleBackdropPointerDown = (event) => {
    if (isProgress || event.target !== event.currentTarget) {
      return
    }

    onClose()
  }

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
    (transparentEnabled) => {
      const exportMode = transparentEnabled ? 'transparent' : 'composite'

      // Only the first switch into transparent mode auto-prefills the imported
      // video span; after that, manual edits stay intact until the dialog closes.
      if (transparentEnabled && hasImportedVideo && !importedVideoRangePrefilledRef.current) {
        importedVideoRangePrefilledRef.current = true
        onSettingsChange({
          exportMode,
          exportRange: {
            ...(settings?.exportRange || {}),
            ...getImportedVideoExportRange(importedVideoDuration, videoSyncOffsetSeconds),
          },
        })
        return
      }

      onSettingsChange({ exportMode })
    },
    [hasImportedVideo, importedVideoDuration, onSettingsChange, settings?.exportRange, videoSyncOffsetSeconds],
  )

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

  return {
    availableCodecs,
    config,
    containerFps,
    dialogTitle: exportMode === 'composite' ? 'Composite Video Export Settings' : 'Transparent Export Settings',
    exportMode,
    fpsMode,
    handleAccelerationChange,
    handleApplyImportedVideoRange,
    handleBackdropPointerDown,
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
    onSettingsChange,
    OUTPUT_FORMATS,
    phase,
    platformOs,
    renderProgress,
    renderStartDisabled,
    renderingVideo,
    resolutionMismatch,
    selectedAccelerationOptions,
    selectedAccelerationValue,
    selectedCodecIsMp4,
    selectedOutputFormatValue,
    settings,
    showExportModeOverride: hasImportedVideo,
    showExportRangeSettings: exportMode !== 'composite',
    updateRateOptions,
  }
}
