/**
 * Preview proxies of the program's transitions, keyed by their content key (see
 * `lib/video-program/transition-proxies`). Session state: not undoable, not saved.
 * A proxy is `rendering`, `ready` with its served URL, or `failed`.
 */

export const createTransitionProxySlice = (set) => ({
  transitionProxies: {},

  setTransitionProxy: (key, proxy) => {
    if (!['rendering', 'ready', 'failed'].includes(proxy.status)) throw new Error(`Unknown transition proxy status: ${proxy.status}`)
    set((draft) => {
      draft.transitionProxies[key] = proxy
    })
  },

  removeTransitionProxy: (key) => {
    set((draft) => {
      delete draft.transitionProxies[key]
    })
  },

  /** Forgets every proxy after their registrations were cleared with the other preview videos. */
  clearTransitionProxies: () => {
    set({ transitionProxies: {} })
  },
})
