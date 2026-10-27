/**
 * Owns the import dialog: probes the selected files, keeps a draft of the whole
 * video collection (videos already imported and new ones) that the dialog edits
 * with the same recipes as the store, and commits the reviewed import.
 */

import { useMemo, useRef, useState } from 'react'
import { produce } from 'immer'
import { useTranslation } from 'react-i18next'
import { formatCombinedExportIssue } from '@/features/render-video'
import { transitionLabel } from '@/features/transitions'
import { deriveVideoSyncControls } from '@/features/toolbar/utils/videoSyncControls'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import {
  moveVideoNextTo,
  placeAutomaticVideos,
  resetCreationTimes,
  resetVideoOffset,
  setVideoOffset,
  setVideoSyncTimezoneMode,
  shiftVideoOffsets,
  applyFilenameCreationTimes,
} from '@/lib/video-program/collection-edits'
import { nextImportOrder } from '@/lib/video-program/imported-video'
import { NO_TRANSITION } from '@/lib/video-program/transition-catalog'
import { formatSecondsClock } from '@/lib/time-format'
import { deriveCompatibility } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'
import { buildImportReview, buildShownCollection, openImportReview, resolveImportSwitches, switchOnWaitingVideo } from '../utils/importReview'
import { formatVideoDetails } from '../utils/videoFormat'
import { commitReviewedImport, probeVideo } from '../videoImportOperations'

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error)
}

function snapshotCollection(state) {
  const { importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary } = state
  return { importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary }
}

/**
 * @param {{onImported: (videos: Array<object>) => void}} options Called with the new videos once they joined the collection.
 * @returns {{start: (paths: Array<string>) => Promise<void>, dialog: object}} Entry point and the dialog model.
 */
export default function useVideoImportReview({ onImported }) {
  const { t } = useTranslation()
  const renderFps = useStore((state) => state.renderSettings.fps)
  const [review, setReviewState] = useState(null)
  const reviewRef = useRef(null)
  const setReview = (next) => {
    reviewRef.current = next
    setReviewState(next)
  }

  /** Probes the files and opens the review of the whole collection with them. */
  const start = async (paths) => {
    if (reviewRef.current !== null) return
    // Cancelling while the files are read discards this review.
    const token = Symbol('import review')
    setReview({ phase: 'probing', fileCount: paths.length, token })
    const state = useStore.getState()
    const knownPaths = new Set(state.importedVideos.map((video) => video.path))
    let importOrder = nextImportOrder(state.importedVideos)
    const failures = []
    const candidates = []
    for (const path of paths) {
      if (knownPaths.has(path)) {
        failures.push({ path, message: t('video-preview.alreadyImported', 'Already imported') })
        continue
      }
      knownPaths.add(path)
      candidates.push({ path, importOrder })
      importOrder += 1
    }
    const probes = await Promise.allSettled(candidates.map(({ path, importOrder }) => probeVideo(path, { importOrder })))
    const videos = []
    probes.forEach((probe, index) => {
      if (probe.status === 'fulfilled') videos.push(probe.value)
      else failures.push({ path: candidates[index].path, message: errorMessage(probe.reason) })
    })
    if (reviewRef.current?.token !== token) return
    const current = useStore.getState()
    setReview({
      token,
      phase: 'reviewing',
      ...openImportReview(snapshotCollection(current), videos),
      failures,
      transitionDefaults: { ...current.transitionDefaults },
      adoptsFormat: current.importedVideos.length === 0,
      error: null,
    })
  }

  /** Applies a change of the review state; returns the reason when the change is rejected. */
  const update = (makeChanges) => {
    const current = reviewRef.current
    try {
      setReview({ ...current, ...makeChanges(current), phase: 'reviewing', error: null })
      return null
    } catch (error) {
      setReview({ ...current, error: error.message })
      return error.message
    }
  }
  /** Applies a collection recipe and switch changes to the review. */
  const change = (recipe, args, switchChanges = []) =>
    update((current) => ({
      collection: produce(current.collection, (draft) => void recipe(draft, ...args)),
      switchOverrides: new Map([...current.switchOverrides, ...switchChanges]),
    }))
  const edit = (recipe, ...args) => change(recipe, args)
  // Editing a video's sync keeps it switched on whatever the defaults become.
  const editVideo = (videoId, recipe, ...args) => change(recipe, [videoId, ...args], [[videoId, true]])

  const derived = useMemo(() => {
    if (review === null || review.phase === 'probing') return null
    const { includedIds, undeterminedIds, unplacedIds, blockedIds, resolution } = resolveImportSwitches(review)
    const { newIds, transitionDefaults } = review
    const shown = buildShownCollection({ collection: review.collection, includedIds, unplacedIds })
    return {
      includedIds,
      resolution,
      shown,
      ...buildImportReview({ collection: shown, newIds, includedIds, undeterminedIds, unplacedIds, blockedIds, transitionDefaults }),
    }
  }, [review])

  const confirm = async () => {
    const current = reviewRef.current
    // Videos switched on far from the activity (or the other videos) stretch the timeline: ask first.
    if (current.phase === 'reviewing' && derived.rows.some((row) => row.isIncluded && row.isFar)) {
      setReview({ ...current, phase: 'confirming', error: null })
      return
    }
    const reviewed = derived.included
    const adoptedFormat = current.adoptsFormat ? deriveCompatibility(reviewed.importedVideos).reference : null
    setReview({ ...current, phase: 'importing', error: null })
    try {
      const { importedVideos, videoEdits, videoSyncTimezoneMode } = reviewed
      const added = await commitReviewedImport({ importedVideos, videoEdits, videoSyncTimezoneMode }, current.transitionDefaults, adoptedFormat)
      setReview(null)
      onImported(added)
    } catch (error) {
      setReview({ ...current, phase: 'reviewing', error: errorMessage(error) })
    }
  }

  if (review === null || derived === null) {
    return {
      start,
      dialog: { open: review !== null, phase: review?.phase ?? null, fileCount: review?.fileCount ?? 0, onCancel: () => setReview(null) },
    }
  }

  const { rows, windowKind, issues, included, timeline, includedIds, resolution, shown } = derived
  const clips = review.collection.importedVideos
  // Switched-off videos are not on the timeline, so a positioned video moves among the switched-on ones.
  const includedRows = rows.filter((row) => row.isIncluded)
  const moveTo = (videoId, index) => {
    // A positioned video sits right after the row above its new place, or before the first row.
    const others = includedRows.filter((row) => row.id !== videoId)
    // Moving only places the video: its switch keeps following the defaults.
    change(moveVideoNextTo, [videoId, index === 0 ? others[0].id : others[index - 1].id, index === 0 ? 'before' : 'after'])
  }
  const { type, durationSeconds } = review.transitionDefaults
  const setIncluded = (row, isIncluded) =>
    // A waiting video switched on joins the others where it waited; other switches are set by the user.
    row.isWaiting ? update((current) => switchOnWaitingVideo(current, row.id, shown, includedIds)) : change(() => {}, [], [[row.id, isIncluded]])
  const farRows = includedRows.filter((row) => row.isFar)
  const sharedOffsetControls = deriveVideoSyncControls(included)
  const setTransitionDefaults = (changes) =>
    setReview({ ...reviewRef.current, transitionDefaults: { ...reviewRef.current.transitionDefaults, ...changes } })

  return {
    start,
    dialog: {
      open: true,
      phase: review.phase,
      fileCount: review.newIds.size + review.failures.length,
      windowKind,
      resolutionLabel: resolution === null ? null : `${resolution.width}×${resolution.height}`,
      rows: rows.map((row) => {
        // A switched-off video keeps its sync and position until it is switched on again.
        const index = includedRows.indexOf(row)
        const canMove = row.isPositioned && row.isIncluded
        return {
          ...row,
          formatLabel: formatVideoDetails(row, t),
          onOffsetCommit: (seconds) => editVideo(row.id, setVideoOffset, seconds),
          onOffsetReset: () => editVideo(row.id, resetVideoOffset),
          onMoveUp: canMove && index > 0 ? () => moveTo(row.id, index - 1) : null,
          onMoveDown: canMove && index < includedRows.length - 1 ? () => moveTo(row.id, index + 1) : null,
          // The overlay is laid out for one resolution: a video with another one cannot be switched on.
          onIncludedChange: row.hasOtherResolution ? null : (isIncluded) => setIncluded(row, isIncluded),
        }
      }),
      failures: review.failures.map(({ path, message }) => ({ key: path, name: filenameFromSelectedPath(path), message })),
      importCount: [...review.newIds].filter((videoId) => includedIds.has(videoId)).length,
      issues: issues.map((issue) => formatCombinedExportIssue(issue, clips, t)),
      farConfirmation:
        review.phase === 'confirming'
          ? {
              names: farRows.map((row) => row.filename),
              from: formatSecondsClock(timeline.fromSecond),
              to: formatSecondsClock(timeline.toSecond),
            }
          : null,
      sync: {
        ...deriveVideoSyncControls(review.collection),
        // The shared offset is the start of the first video switched on.
        sharedOffsetSeconds: sharedOffsetControls.sharedOffsetSeconds,
        sharedOffsetFps: sharedOffsetControls.sharedOffsetFps,
        // The rows show their own warnings; manual sync needs an imported video.
        videoSyncWarning: null,
        openManualVideoSync: null,
        computeVideoSync: () => edit(placeAutomaticVideos),
        commitSharedOffset: (seconds) => edit(shiftVideoOffsets, seconds - sharedOffsetControls.sharedOffsetSeconds),
        resetVideoCreationTimes: () => edit(resetCreationTimes),
        setVideoCreationTimesFromFilenames: () => edit(applyFilenameCreationTimes),
        setVideoSyncTimezoneMode: (mode) => edit(setVideoSyncTimezoneMode, mode),
      },
      transition: {
        type,
        typeLabel: type === NO_TRANSITION ? transitionLabel({ type, durationSeconds: 0 }, t) : type,
        previewType: type === NO_TRANSITION ? null : type,
        durationSeconds,
        durationFps: { num: renderFps, den: 1 },
        onTypeChange: (value) => setTransitionDefaults({ type: value }),
        onDurationCommit: (seconds) => setTransitionDefaults({ durationSeconds: seconds }),
      },
      error: review.error,
      onCancel: () => setReview(null),
      onConfirm: confirm,
      onBack: () => setReview({ ...reviewRef.current, phase: 'reviewing' }),
    },
  }
}
