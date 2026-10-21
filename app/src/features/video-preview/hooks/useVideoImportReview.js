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
import { filenameFromSelectedPath } from '@/lib/utils'
import {
  addVideos,
  findUndeterminedVideoIds,
  moveVideoNextTo,
  placeAutomaticVideos,
  removeVideo,
  resetCreationTimes,
  resetVideoOffset,
  setVideoOffset,
  setVideoSyncTimezoneMode,
  shiftVideoOffsets,
  applyFilenameCreationTimes,
} from '@/lib/video-program/collection-edits'
import { nextImportOrder } from '@/lib/video-program/imported-video'
import { NO_TRANSITION } from '@/lib/video-program/transition-catalog'
import { deriveCompatibility } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'
import { buildImportReview } from '../utils/importReview'
import { formatVideoFormat } from '../utils/videoFormat'
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
      collection: produce(snapshotCollection(current), (draft) => {
        if (videos.length > 0) addVideos(draft, videos)
      }),
      newIds: new Set(videos.map((video) => video.id)),
      failures,
      transitionDefaults: { ...current.transitionDefaults },
      adoptsFormat: current.importedVideos.length === 0,
      error: null,
    })
  }

  /** Applies a collection recipe to the draft; returns the reason when it is rejected. */
  const edit = (recipe, ...args) => {
    const current = reviewRef.current
    try {
      setReview({ ...current, collection: produce(current.collection, (draft) => void recipe(draft, ...args)), error: null })
      return null
    } catch (error) {
      setReview({ ...current, error: error.message })
      return error.message
    }
  }

  const derived = useMemo(() => {
    if (review === null || review.phase === 'probing') return null
    const undeterminedIds = findUndeterminedVideoIds(review.collection)
    return buildImportReview({ collection: review.collection, newIds: review.newIds, undeterminedIds, transitionDefaults: review.transitionDefaults })
  }, [review])

  const confirm = async () => {
    const current = reviewRef.current
    const reviewed = produce(current.collection, (draft) => {
      for (const video of current.collection.importedVideos) if (!derived.joiningIds.has(video.id)) removeVideo(draft, video.id)
    })
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

  const { rows, windowKind, issues, joiningIds } = derived
  const clips = review.collection.importedVideos
  const moveTo = (videoId, index) => {
    // A positioned video sits right after the row above its new place, or before the first row.
    const others = rows.filter((row) => row.id !== videoId)
    edit(moveVideoNextTo, videoId, index === 0 ? others[0].id : others[index - 1].id, index === 0 ? 'before' : 'after')
  }
  const { type, durationSeconds } = review.transitionDefaults
  const setTransitionDefaults = (changes) =>
    setReview({ ...reviewRef.current, transitionDefaults: { ...reviewRef.current.transitionDefaults, ...changes } })

  return {
    start,
    dialog: {
      open: true,
      phase: review.phase,
      fileCount: review.newIds.size + review.failures.length,
      windowKind,
      rows: rows.map((row, index) => ({
        ...row,
        formatLabel: formatVideoFormat(row, t),
        onOffsetCommit: (seconds) => edit(setVideoOffset, row.id, seconds),
        onOffsetReset: () => edit(resetVideoOffset, row.id),
        onMoveUp: row.isPositioned && !row.isFirst ? () => moveTo(row.id, index - 1) : null,
        onMoveDown: row.isPositioned && !row.isLast ? () => moveTo(row.id, index + 1) : null,
        onRemove: row.verdict === 'outside' ? () => edit(removeVideo, row.id) : null,
      })),
      failures: review.failures.map(({ path, message }) => ({ key: path, name: filenameFromSelectedPath(path), message })),
      importCount: [...review.newIds].filter((videoId) => joiningIds.has(videoId)).length,
      issues: issues.map((issue) => formatCombinedExportIssue(issue, clips, t)),
      sync: {
        ...deriveVideoSyncControls(review.collection),
        // The rows show their own warnings; manual sync needs an imported video.
        videoSyncWarning: null,
        openManualVideoSync: null,
        computeVideoSync: () => edit(placeAutomaticVideos),
        commitSharedOffset: (seconds) => edit(shiftVideoOffsets, seconds - deriveVideoSyncControls(reviewRef.current.collection).sharedOffsetSeconds),
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
    },
  }
}
