/** User queue choices and retained native execution snapshots. Inspection stays transient. */
function requireFolder(path) {
  if (path !== null && (typeof path !== 'string' || path.trim() === '')) throw new Error('Batch folder must be a nonempty path or null')
}

function requireItem(state, id) {
  const item = state.batchQueue.find((candidate) => candidate.id === id)
  if (!item) throw new Error(`Unknown batch queue item: ${id}`)
  return item
}

export function createBatchRenderSlice(set) {
  return {
    batchVideoFolder: null,
    batchOutputFolder: null,
    batchQueue: [],
    batchSnapshot: null,

    setBatchVideoFolder: (path) => {
      requireFolder(path)
      set({ batchVideoFolder: path })
    },
    setBatchOutputFolder: (path) => {
      requireFolder(path)
      set({ batchOutputFolder: path })
    },
    setBatchQueueFromPaths: (paths) => {
      if (!Array.isArray(paths) || paths.some((path) => typeof path !== 'string' || path.trim() === '') || new Set(paths).size !== paths.length) {
        throw new Error('Batch queue requires distinct nonempty source paths')
      }
      set((state) => {
        const previous = new Map(state.batchQueue.map((item) => [item.path, item]))
        state.batchQueue = paths.map((path) => ({
          id: path,
          path,
          filename: path.split(/[/\\]/).at(-1),
          skipOverlay: previous.get(path)?.skipOverlay ?? false,
        }))
      })
    },
    removeBatchQueueItem: (id) =>
      set((state) => {
        requireItem(state, id)
        state.batchQueue = state.batchQueue.filter((item) => item.id !== id)
      }),
    clearBatchQueue: () => set({ batchQueue: [], batchVideoFolder: null }),
    setBatchItemSkipOverlay: (id, skipOverlay) => {
      if (typeof skipOverlay !== 'boolean') throw new Error('Activity overlay suppression must be boolean')
      set((state) => {
        requireItem(state, id).skipOverlay = skipOverlay
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
