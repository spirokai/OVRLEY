/**
 * Separate-file export of every imported video: the output folder and retained
 * native execution snapshots. Inspection stays transient.
 */
function requireFolder(path) {
  if (path !== null && (typeof path !== 'string' || path.trim() === '')) throw new Error('Batch folder must be a nonempty path or null')
}

export function createBatchRenderSlice(set) {
  return {
    batchOutputFolder: null,
    batchSnapshot: null,

    setBatchOutputFolder: (path) => {
      requireFolder(path)
      set({ batchOutputFolder: path })
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
