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

import { useCallback, useState } from 'react'
import { cancelRender } from '@/api/backend'
import { getFpsModeValue, normalizeUpdateRateForFps, PRESET_FPS_VALUES, sanitizeIntegerFps } from '@/lib/update-rate'
import { OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getExportCodecForSelection,
  getFirstAvailableAcceleration,
  getVisibleAccelerationOptions,
  isOutputFormatAvailable,
} from '../utils/codecUtils'
import useRenderVideoDerivedState from './useRenderVideoDerivedState'
import useRenderVideoEffects from './useRenderVideoEffects'

export default function useRenderVideoDialogState({ phase, settings, onSettingsChange, onClose, onConfirm }) {
  const derived = useRenderVideoDerivedState({ settings })

  // The dialog only owns whether the user deliberately opened the custom FPS
  // path for the current committed FPS. The FPS value itself remains derived
  // from the committed draft settings.
  const [customFpsAnchor, setCustomFpsAnchor] = useState(null)
  const fpsMode = customFpsAnchor !== null && Number(settings?.fps) === customFpsAnchor ? 'custom' : getFpsModeValue(settings?.fps)

  useRenderVideoEffects({
    settings,
    derivedState: derived,
    onSettingsChange,
  })

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

  const handleOutputFormatChange = (value) => {
    const format = OUTPUT_FORMATS_BY_VALUE[value]
    if (!format) {
      return
    }

    const acceleration =
      getVisibleAccelerationOptions(format, derived.platformOs, derived.availableCodecs).find(
        (option) => option.value === derived.selectedAccelerationValue && option.available,
      ) || getFirstAvailableAcceleration(format, derived.platformOs, derived.availableCodecs)

    if (!acceleration) {
      return
    }

    const nextExportCodec = getExportCodecForSelection(format.value, acceleration.value)
    const nextIsMp4Codec = format.group === 'mp4'

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: acceleration.value,
      exportBitrate: nextIsMp4Codec ? derived.defaultBitrateForCodec(nextExportCodec) : undefined,
    })
  }

  const handleFpsModeChange = useCallback(
    (value) => {
      if (value === 'custom') {
        setCustomFpsAnchor(Number(settings?.fps))
        return
      }

      setCustomFpsAnchor(null)
      const fps = sanitizeIntegerFps(value)
      onSettingsChange({
        fps,
        updateRate: normalizeUpdateRateForFps(fps, settings?.updateRate),
      })
    },
    [onSettingsChange, settings?.fps, settings?.updateRate],
  )

  const handleCustomFpsChange = useCallback(
    (rawValue) => {
      const fps = sanitizeIntegerFps(rawValue)
      setCustomFpsAnchor(PRESET_FPS_VALUES.includes(fps) ? null : fps)
      onSettingsChange({
        fps,
        updateRate: normalizeUpdateRateForFps(fps, settings?.updateRate),
      })
    },
    [onSettingsChange, settings?.updateRate],
  )

  const handleAccelerationChange = (value) => {
    const nextExportCodec = getExportCodecForSelection(derived.selectedOutputFormatValue, value)
    if (!nextExportCodec) {
      return
    }

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: value,
      exportBitrate: derived.selectedCodecIsMp4 ? derived.defaultBitrateForCodec(nextExportCodec) : undefined,
    })
  }

  return {
    availableCodecs: derived.availableCodecs,
    config: derived.config,
    containerFps: derived.containerFps,
    fpsMode,
    handleAccelerationChange,
    handleBackdropPointerDown,
    handleCancel,
    handleCustomFpsChange,
    handleFpsModeChange,
    handleOutputFormatChange,
    hasImportedVideo: derived.hasImportedVideo,
    importedVideoFps: derived.importedVideoFps,
    importedVideoResolution: derived.importedVideoResolution,
    isProgress,
    isOutputFormatAvailable,
    onClose,
    onConfirm,
    onSettingsChange,
    OUTPUT_FORMATS,
    phase,
    platformOs: derived.platformOs,
    renderProgress: derived.renderProgress,
    renderStartDisabled: derived.renderStartDisabled,
    renderingVideo: derived.renderingVideo,
    resolutionMismatch: derived.resolutionMismatch,
    selectedAccelerationOptions: derived.selectedAccelerationOptions,
    selectedAccelerationValue: derived.selectedAccelerationValue,
    selectedCodecIsMp4: derived.selectedCodecIsMp4,
    selectedOutputFormatValue: derived.selectedOutputFormatValue,
    settings,
    updateRateOptions: derived.updateRateOptions,
  }
}
