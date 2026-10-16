/**
 * Keeps a preview proxy for every transition of the program.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import { useTransitionProxyJobs } from '@/hooks/useVideoTimeline'
import { pickNextTransitionProxyJob } from '@/lib/video-program/transition-proxies'
import useStore from '@/store/useStore'

/**
 * Renders missing proxies one at a time, the one nearest the playhead first, and
 * releases proxies no join needs any more. A failed proxy is not retried until
 * its join changes; the preview then shows a hard cut.
 */
export function useTransitionProxyQueue() {
  const jobs = useTransitionProxyJobs()
  const proxies = useStore((state) => state.transitionProxies)

  useEffect(() => {
    const { setTransitionProxy, removeTransitionProxy, setErrorMessage } = useStore.getState()
    const release = (importId) => void backend.releaseTransitionProxy(importId).catch((error) => setErrorMessage(error.message))
    const wanted = new Set(jobs.map((job) => job.key))
    for (const [key, proxy] of Object.entries(proxies)) {
      if (wanted.has(key) || proxy.status === 'rendering') continue
      removeTransitionProxy(key)
      if (proxy.status === 'ready') release(proxy.importId)
    }
    if (Object.values(proxies).some((proxy) => proxy.status === 'rendering')) return
    const job = pickNextTransitionProxyJob(jobs, proxies, useStore.getState().selectedSecond)
    if (job === null) return
    setTransitionProxy(job.key, { status: 'rendering' })
    void backend
      .renderTransitionProxy(job.request)
      .then(({ importId, previewUrl }) => {
        // Proxies forgotten meanwhile (all preview videos cleared) release their late registration.
        if (useStore.getState().transitionProxies[job.key]?.status !== 'rendering') release(importId)
        else setTransitionProxy(job.key, { status: 'ready', importId, url: previewUrl })
      })
      .catch((error) => {
        console.error('[useTransitionProxyQueue] Transition preview failed', error)
        if (useStore.getState().transitionProxies[job.key]?.status === 'rendering') {
          setTransitionProxy(job.key, { status: 'failed', error: error.message })
        }
      })
  }, [jobs, proxies])
}
