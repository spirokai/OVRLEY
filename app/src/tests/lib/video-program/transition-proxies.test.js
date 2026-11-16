import { describe, expect, test } from 'vite-plus/test'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import {
  listShownTransitionJoinKeys,
  listTransitionProxyJobs,
  pickNextTransitionProxyJob,
  resolveContinuation,
  resolveUpcomingTransition,
} from '@/lib/video-program/transition-proxies'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const fade = { type: 'fade', durationSeconds: 2, splitGapThresholdSeconds: 2 }

function timeline(edit = () => {}) {
  const { importedVideos, videoEdits } = videoStoreState(
    [
      createTestVideo({ id: 'ride', path: '/videos/ride.mp4', durationSeconds: 60, width: 3840, height: 2160 }),
      createTestVideo({ id: 'climb', path: '/videos/climb.mp4', durationSeconds: 30, hasAudio: false, importOrder: 1 }),
    ],
    { ride: 0, climb: 100 },
  )
  edit(videoEdits)
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: fade })
}

describe('transition windows', () => {
  test('blend the last seconds of the outgoing piece into the incoming piece at its own time', () => {
    const { clips, program } = timeline()
    const ride = program.pieces[0]
    const jobs = listTransitionProxyJobs(program, clips)
    const proxies = { [jobs[0].key]: { status: 'ready', url: 'http://127.0.0.1:4321/video/proxy' } }
    // The piece ending with the transition already knows its proxy, ahead of the window.
    expect(resolveUpcomingTransition(program, jobs, proxies, 30)).toEqual({
      proxyStatus: 'ready',
      proxyUrl: 'http://127.0.0.1:4321/video/proxy',
      isInWindow: false,
      elapsedSeconds: 0,
    })
    expect(resolveUpcomingTransition(program, jobs, proxies, 59)).toMatchObject({ isInWindow: true, elapsedSeconds: expect.closeTo(1, 5) })
    expect(resolveUpcomingTransition(program, jobs, {}, 59)).toMatchObject({ proxyStatus: 'queued', proxyUrl: null })
    // The last piece ends without a transition, and a gap lies outside every piece.
    expect(resolveUpcomingTransition(program, jobs, proxies, 110)).toBeNull()
    expect(resolveUpcomingTransition(program, jobs, proxies, 80)).toBeNull()
    // Playback continues after the incoming start the shown transition played.
    expect(resolveContinuation(program, ride.id, new Set(['climb:start']))).toEqual({ piece: program.pieces[1], second: 102 })
    expect(resolveContinuation(program, program.pieces[1].id, new Set(['climb:start']))).toBeNull()
    expect(() => resolveContinuation(program, 'missing', new Set())).toThrow('Unknown program piece')
  })

  test('without a shown proxy the preview cuts hard and the incoming piece starts from its first frame', () => {
    const { clips, program } = timeline()
    const jobs = listTransitionProxyJobs(program, clips)
    for (const proxies of [{}, { [jobs[0].key]: { status: 'rendering' } }, { [jobs[0].key]: { status: 'failed' } }]) {
      const shown = listShownTransitionJoinKeys(jobs, proxies)
      expect(resolveContinuation(program, program.pieces[0].id, shown)).toEqual({ piece: program.pieces[1], second: 100 })
    }
    expect(listShownTransitionJoinKeys(jobs, { [jobs[0].key]: { status: 'ready' } })).toEqual(new Set(['climb:start']))
  })

  test('hard cuts continue at the next piece and have no window', () => {
    const { clips, program } = timeline((edits) => {
      edits.climb.segments.start.transitionIn = { type: 'none', durationSeconds: 0 }
    })
    expect(resolveUpcomingTransition(program, listTransitionProxyJobs(program, clips), {}, 59.5)).toBeNull()
    expect(resolveContinuation(program, program.pieces[0].id, new Set()).second).toBe(100)
  })
})

describe('transition proxy jobs', () => {
  test('render the missing proxy nearest the playhead first', () => {
    const near = { key: 'near', windowStartSecond: 10, request: { durationSeconds: 2 } }
    const far = { key: 'far', windowStartSecond: 100, request: { durationSeconds: 2 } }
    expect(pickNextTransitionProxyJob([far, near], {}, 0)).toBe(near)
    expect(pickNextTransitionProxyJob([far, near], {}, 95)).toBe(far)
    expect(pickNextTransitionProxyJob([far, near], { far: { status: 'failed' } }, 95)).toBe(near)
    expect(pickNextTransitionProxyJob([far, near], { far: { status: 'ready' }, near: { status: 'rendering' } }, 0)).toBeNull()
  })
})
