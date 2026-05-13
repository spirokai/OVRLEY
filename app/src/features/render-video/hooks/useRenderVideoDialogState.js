/**
 * Container hook for RenderVideoDialog.
 * Orchestrates derived state, side effects, and event handlers.
 * The component receives a single object with everything it needs for rendering.
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
import { OUTPUT_FORMATS, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getExportCodecForSelection,
  getFirstAvailableAcceleration,
  getVisibleAccelerationOptions,
  isOutputFormatAvailable,
} from '../utils/codecUtils'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import useRenderVideoDerivedState from './useRenderVideoDerivedState'
import useRenderVideoEffects from './useRenderVideoEffects'

export default function useRenderVideoDialogState({ phase, settings, onSettingsChange, onClose, onConfirm }) {
  // Derived state (store selectors + computed values)
  const derived = useRenderVideoDerivedState({ settings })

  // Local UI state
  const [fpsMode, setFpsMode] = useState([24, 30, 60].includes(settings?.fps) ? settings.fps.toString() : 'custom')

  // Side effects (sync props/store → local state, auto-select codecs)
  useRenderVideoEffects({
    settings,
    derivedState: derived,
    onSettingsChange,
    setFpsMode,
  })

  // Cancel handler — sends cancel-render IPC to the backend
  const handleCancel = useCallback(async () => {
    await cancelRender()
  }, [])

  // Backdrop click to close — closes the dialog when clicking outside, blocked while render is in progress
  const isProgress = phase === 'progress'
  const handleBackdropPointerDown = (event) => {
    if (isProgress || event.target !== event.currentTarget) {
      return
    }
    onClose()
  }

  // Codec / output format change handler
  const handleOutputFormatChange = (value) => {
    const format = OUTPUT_FORMATS_BY_VALUE[value]
    if (!format) return

    const acceleration =
      getVisibleAccelerationOptions(format, derived.platformOs, derived.availableCodecs).find(
        (option) => option.value === derived.selectedAccelerationValue && option.available,
      ) || getFirstAvailableAcceleration(format, derived.platformOs, derived.availableCodecs)

    if (!acceleration) return

    const nextExportCodec = getExportCodecForSelection(format.value, acceleration.value)
    const nextIsMp4Codec = format.group === 'mp4'

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: acceleration.value,
      exportBitrate: nextIsMp4Codec ? derived.defaultBitrateForCodec(nextExportCodec) : undefined,
    })
  }

  // FPS selection handler
  const handleFpsModeChange = useCallback(
    (value) => {
      setFpsMode(value)
      if (value !== 'custom') {
        const fps = sanitizeIntegerFps(value)
        onSettingsChange({
          fps,
          updateRate: normalizeUpdateRateForFps(fps, settings?.updateRate),
        })
      }
    },
    [onSettingsChange, settings?.updateRate],
  )

  // Custom FPS input handler
  const handleCustomFpsChange = useCallback(
    (rawValue) => {
      const fps = sanitizeIntegerFps(rawValue)
      onSettingsChange({
        fps,
        updateRate: normalizeUpdateRateForFps(fps, settings?.updateRate),
      })
    },
    [onSettingsChange, settings?.updateRate],
  )

  // Hardware acceleration change handler
  const handleAccelerationChange = (value) => {
    const nextExportCodec = getExportCodecForSelection(derived.selectedOutputFormatValue, value)
    if (!nextExportCodec) return

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
