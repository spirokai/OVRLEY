import { describe, expect, test } from 'vitest'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import {
  listShownTransitionJoinKeys,
  listTransitionProxyJobs,
  pickNextTransitionProxyJob,
  resolveContinuation,
  resolveTransitionWindow,
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
    const { program } = timeline()
    const ride = program.pieces[0]
    expect(resolveTransitionWindow(program, 57.9)).toBeNull()
    expect(resolveTransitionWindow(program, 59)).toMatchObject({
      join: { key: 'clip:climb' },
      progress: expect.closeTo(0.5, 5),
      elapsedSeconds: expect.closeTo(1, 5),
      incomingSecond: expect.closeTo(101, 5),
    })
    // Playback continues after the incoming start the shown transition played.
    expect(resolveContinuation(program, ride.id, new Set(['clip:climb']))).toEqual({ piece: program.pieces[1], second: 102 })
    expect(resolveContinuation(program, program.pieces[1].id, new Set(['clip:climb']))).toBeNull()
    expect(() => resolveContinuation(program, 'missing', new Set())).toThrow('Unknown program piece')
  })

  test('without a shown proxy the preview cuts hard and the incoming piece starts from its first frame', () => {
    const { clips, program } = timeline()
    const jobs = listTransitionProxyJobs(program, clips)
    for (const proxies of [{}, { [jobs[0].key]: { status: 'rendering' } }, { [jobs[0].key]: { status: 'failed' } }]) {
      const shown = listShownTransitionJoinKeys(jobs, proxies)
      expect(resolveContinuation(program, program.pieces[0].id, shown)).toEqual({ piece: program.pieces[1], second: 100 })
    }
    expect(listShownTransitionJoinKeys(jobs, { [jobs[0].key]: { status: 'ready' } })).toEqual(new Set(['clip:climb']))
  })

  test('hard cuts continue at the next piece and have no window', () => {
    const { program } = timeline((edits) => {
      edits.climb.transitionIn = { type: 'none', durationSeconds: 0 }
    })
    expect(resolveTransitionWindow(program, 59.5)).toBeNull()
    expect(resolveContinuation(program, program.pieces[0].id, new Set()).second).toBe(100)
  })
})

describe('transition proxy jobs', () => {
  test('describe every xfade join by its content, at preview size', () => {
    const { clips, program } = timeline()
    const [job] = listTransitionProxyJobs(program, clips)
    expect(job.joinKey).toBe('clip:climb')
    expect(job.windowStartSecond).toBeCloseTo(58, 5)
    expect(job.request).toEqual({
      outgoing: { path: '/videos/ride.mp4', startSecond: expect.closeTo(58, 5), hasAudio: true },
      incoming: { path: '/videos/climb.mp4', startSecond: 0, hasAudio: false },
      transition: 'fade',
      durationSeconds: 2,
      width: 640,
      height: 360,
      fpsNum: 30,
      fpsDen: 1,
    })
    expect(job.key).toBe(JSON.stringify(job.request))
    // Any change to what the proxy shows changes its key.
    const moved = timeline((edits) => {
      edits.climb.transitionIn = { type: 'wipeleft', durationSeconds: 2 }
    })
    expect(listTransitionProxyJobs(moved.program, moved.clips)[0].key).not.toBe(job.key)
    const cut = timeline((edits) => {
      edits.climb.transitionIn = { type: 'none', durationSeconds: 0 }
    })
    expect(listTransitionProxyJobs(cut.program, cut.clips)).toEqual([])
  })

  test('render the missing proxy nearest the playhead first', () => {
    const near = { key: 'near', windowStartSecond: 10, request: { durationSeconds: 2 } }
    const far = { key: 'far', windowStartSecond: 100, request: { durationSeconds: 2 } }
    expect(pickNextTransitionProxyJob([far, near], {}, 0)).toBe(near)
    expect(pickNextTransitionProxyJob([far, near], {}, 95)).toBe(far)
    expect(pickNextTransitionProxyJob([far, near], { far: { status: 'failed' } }, 95)).toBe(near)
    expect(pickNextTransitionProxyJob([far, near], { far: { status: 'ready' }, near: { status: 'rendering' } }, 0)).toBeNull()
  })
})
