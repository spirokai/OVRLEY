/**
 * Owns custom export-range marker state for the player timeline.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatExportRangeTime } from '@/features/overlay-editor/utils/exportRange'
import useStore from '@/store/useStore'
import { clamp } from '@/lib/utils'
import { clampExportRangeMarkerSecond } from '../utils/timelineGeometry'

function getMarkerLabel(marker) {
  return marker === 'from' ? 'Export from' : 'Export to'
}

/**
 * Owns custom export-range state, marker preview, clamping, snapping, and store writes.
 *
 * @param {{ totalDuration: number, timelineMinimum?: number, defaultEndSecond?: number }} options Export timeline inputs.
 * @param {number} options.totalDuration Total playable duration used to clamp markers.
 * @param {number} [options.defaultEndSecond=totalDuration] End used when enabling a new custom range.
 * @returns {object} Export range timeline state and commands.
 */
export default function useExportRangeTimeline({ totalDuration, timelineMinimum = 0, defaultEndSecond = totalDuration }) {
  // Store selector - export range inputs remain the source of truth outside active drag preview.
  const { exportRange, setExportRange } = useStore(
    useShallow((state) => ({
      exportRange: state.exportRange,
      setExportRange: state.setExportRange,
    })),
  )

  // Last-write ref - prevents redundant store writes while dragging to the same snapped time.
  const writtenRangeRef = useRef({
    from: exportRange.from,
    to: exportRange.to,
  })

  // Drag preview - marker movement is visible immediately without committing sidebar/export state.
  const [dragPreview, setDragPreview] = useState(null)

  // Store sync - reset write tracking whenever another UI changes the export range.
  useEffect(() => {
    writtenRangeRef.current = {
      from: exportRange.from,
      to: exportRange.to,
    }
  }, [exportRange])

  // Displayed seconds - preview state temporarily overrides the persisted marker being dragged.
  const isCustom = exportRange.type === 'custom'
  const fromSecond = clamp(exportRange.from, timelineMinimum, totalDuration)
  const toSecond = clamp(exportRange.to, timelineMinimum, totalDuration)
  const displayedFromSecond = dragPreview?.marker === 'from' ? dragPreview.second : fromSecond
  const displayedToSecond = dragPreview?.marker === 'to' ? dragPreview.second : toSecond

  // Preview command - clamp continuously so markers never cross or leave the timeline while dragging.
  const previewMarker = useCallback(
    (marker, second) => {
      if (!isCustom) return

      const previewSecond = clampExportRangeMarkerSecond({
        marker,
        second,
        fromSecond: displayedFromSecond,
        timelineMinimum,
        toSecond: displayedToSecond,
        totalDuration,
      })

      setDragPreview({
        marker,
        second: previewSecond,
      })
    },
    [displayedFromSecond, displayedToSecond, isCustom, timelineMinimum, totalDuration],
  )

  // Commit command - persist the exact fractional marker position.
  const commitMarker = useCallback(
    (marker, second) => {
      if (!isCustom) return

      const nextSecond = clampExportRangeMarkerSecond({
        marker,
        second,
        fromSecond: displayedFromSecond,
        timelineMinimum,
        toSecond: displayedToSecond,
        totalDuration,
      })
      const field = marker === 'from' ? 'from' : 'to'

      setDragPreview(null)
      if (writtenRangeRef.current[field] === nextSecond) return

      writtenRangeRef.current = {
        ...writtenRangeRef.current,
        [field]: nextSecond,
      }
      setExportRange({ [field]: nextSecond })
    },
    [displayedFromSecond, displayedToSecond, isCustom, setExportRange, timelineMinimum, totalDuration],
  )

  // Cancel command - pointer cancellation drops transient preview without touching store state.
  const cancelMarkerPreview = useCallback(() => {
    setDragPreview(null)
  }, [])

  // Toolbar command - enable the range and set the requested bound to the playhead.
  const setBoundary = useCallback(
    (marker, second) => {
      const currentFromSecond = isCustom ? fromSecond : timelineMinimum
      const currentToSecond = isCustom ? toSecond : defaultEndSecond
      const boundarySecond = clampExportRangeMarkerSecond({
        marker,
        second,
        fromSecond: currentFromSecond,
        timelineMinimum,
        toSecond: currentToSecond,
        totalDuration,
      })

      setDragPreview(null)
      setExportRange({
        type: 'custom',
        from: marker === 'from' ? boundarySecond : currentFromSecond,
        to: marker === 'to' ? boundarySecond : currentToSecond,
      })
    },
    [defaultEndSecond, fromSecond, isCustom, setExportRange, timelineMinimum, toSecond, totalDuration],
  )

  const clear = useCallback(() => {
    setDragPreview(null)
    setExportRange({ type: 'all' })
  }, [setExportRange])

  // Highlight model - lanes use this same range so preview markers and clip shading stay aligned.
  const highlightRange = useMemo(() => {
    if (!isCustom) return null

    return {
      fromSecond: displayedFromSecond,
      toSecond: displayedToSecond,
    }
  }, [displayedFromSecond, displayedToSecond, isCustom])

  // Marker model - presentational surface only receives labels and timeline seconds for visible markers.
  const markers = useMemo(() => {
    if (!isCustom) return []

    return [
      {
        marker: 'from',
        label: getMarkerLabel('from'),
        second: displayedFromSecond,
      },
      {
        marker: 'to',
        label: getMarkerLabel('to'),
        second: displayedToSecond,
      },
    ]
  }, [displayedFromSecond, displayedToSecond, isCustom])

  // Export timeline API - gesture hooks call commands, components render derived marker models.
  return {
    cancelMarkerPreview,
    clear,
    commitMarker,
    fromSecond,
    highlightRange,
    isCustom,
    markers,
    previewMarker,
    rangeLabel: isCustom ? `[${formatExportRangeTime(displayedFromSecond)}-${formatExportRangeTime(displayedToSecond)}]` : null,
    setBoundary,
    toSecond,
  }
}
