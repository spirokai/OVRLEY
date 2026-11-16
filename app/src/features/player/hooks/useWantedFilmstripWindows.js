/**
 * Tells the filmstrip queue which zoom windows the timeline shows, once the view settles.
 */

import { useEffect, useMemo } from 'react'
import useStore from '@/store/useStore'
import { listWantedFilmstripWindows } from '../utils/filmstripTiles'
import { videoClipHeightPx } from './useTimelineClips'

/** Quiet time after the last zoom or pan before new windows are asked for, so a zoom gesture skips the levels it passes. */
export const FILMSTRIP_WINDOW_SETTLE_MS = 200

/**
 * Publishes the zoom windows of the displayed video clips at the current view to the store, where the filmstrip queue
 * (`useVideoFilmstripQueue`) loads them. A view that changes the wanted windows publishes them after
 * `FILMSTRIP_WINDOW_SETTLE_MS` without further change.
 *
 * @param {{clips: object[], viewStart: number, viewEnd: number, widthPx: number}} options Displayed video clips and view.
 */
export default function useWantedFilmstripWindows({ clips, viewStart, viewEnd, widthPx }) {
  const wanted = useMemo(
    () => listWantedFilmstripWindows({ clips, viewStart, viewEnd, widthPx, tileHeightPx: videoClipHeightPx() }),
    [clips, viewStart, viewEnd, widthPx],
  )
  // The windows by value: a pan inside the same windows keeps the key, and nothing is published.
  const wantedKey = JSON.stringify(wanted)

  useEffect(() => {
    const timer = setTimeout(() => useStore.getState().setWantedFilmstripWindows(JSON.parse(wantedKey)), FILMSTRIP_WINDOW_SETTLE_MS)
    return () => clearTimeout(timer)
  }, [wantedKey])

  // A timeline that goes away shows no windows.
  useEffect(() => () => useStore.getState().setWantedFilmstripWindows({}), [])
}
