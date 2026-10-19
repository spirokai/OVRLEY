/**
 * Separate-file export of every imported video: the output folder, per-video
 * overlay choices, and retained native execution snapshots. Inspection stays transient.
 */
function requireFolder(path) {
  if (path !== null && (typeof path !== 'string' || path.trim() === '')) throw new Error('Batch folder must be a nonempty path or null')
}

export function createBatchRenderSlice(set) {
  return {
    batchOutputFolder: null,
    batchSkipOverlay: {}, // video ID → true when that video renders without activity widgets
    batchSnapshot: null,

    setBatchOutputFolder: (path) => {
      requireFolder(path)
      set({ batchOutputFolder: path })
    },
    setBatchItemSkipOverlay: (videoId, skipOverlay) => {
      if (typeof skipOverlay !== 'boolean') throw new Error('Activity overlay suppression must be boolean')
      set((state) => {
        if (skipOverlay) state.batchSkipOverlay[videoId] = true
        else delete state.batchSkipOverlay[videoId]
      })
    },
    acceptBatchSnapshot: (snapshot) => set({ batchSnapshot: snapshot }),
    applyBatchSnapshot: (snapshot) =>
      set((state) => {
        if (state.batchSnapshot === null || snapshot.batchId !== state.batchSnapshot.batchId || snapshot.revision <= state.batchSnapshot.revision)
          return
        state.batchSnapshot = snapshot
      }),
    clearBatchResults: () => set({ batchSnapshot: null }),
  }
}
