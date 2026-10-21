import { describe, expect, test, vi } from 'vitest'
import { buildJoinTransitionModel, joinOverride, transitionLabel } from '@/features/transitions'
import { transitionPreviewStyle } from '@/features/transitions/utils/transitionPreviews'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const t = (key, options) => (typeof options === 'string' ? options : options.defaultValue.replace(/{{(\w+)}}/g, (_, name) => options[name]))
const defaults = { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 2 }
// Frame rate of the fixture videos.
const fps = { num: 30, den: 1 }

function timeline() {
  const { importedVideos, videoEdits } = videoStoreState(
    [
      createTestVideo({ id: 'ride', durationSeconds: 120 }),
      createTestVideo({ id: 'later', path: '/videos/later.mp4', durationSeconds: 60, importOrder: 1 }),
    ],
    { later: 125 },
  )
  videoEdits.ride.deletedParts = [
    { id: 'short', fromSecond: 10, toSecond: 11, transition: null },
    { id: 'long', fromSecond: 30, toSecond: 60, transition: { type: 'wipeleft', durationSeconds: 1.5 } },
  ]
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: defaults })
}

function model(join, clips, setJoinTransition = vi.fn(), isInvalid = false) {
  return buildJoinTransitionModel({ join, fps, override: joinOverride(join, clips), transitionDefaults: defaults, isInvalid, setJoinTransition, t })
}

describe('join transition model', () => {
  test('shows each catalogue transition from its cell of the preview sprite', () => {
    expect(transitionPreviewStyle('fade')).toMatchObject({
      backgroundImage: expect.stringMatching(/transition-previews\.webp/),
      backgroundPosition: '0% 0%',
    })
    expect(transitionPreviewStyle('revealdown')).toMatchObject({ backgroundSize: '800% 800%', backgroundPosition: '14.285714285714285% 100%' })
    expect(() => transitionPreviewStyle('spin')).toThrow(/no preview/)
  })

  test('shows the effective transition and the default each join follows', () => {
    const { clips, program } = timeline()
    const [atShort, atLong, intoLater] = program.joins.map((join) => model(join, clips))
    // One deleted second is within the continuity threshold: a hard cut by default.
    expect(atShort).toMatchObject({
      isAvailable: true,
      value: 'default',
      label: 'No transition',
      defaultLabel: 'Default (No transition)',
      previewType: null,
    })
    expect(atLong).toMatchObject({
      value: 'wipeleft',
      label: 'wipeleft, 1.5 s',
      defaultLabel: 'Default',
      previewType: 'wipeleft',
      durationSeconds: 1.5,
      durationFps: fps,
    })
    expect(intoLater).toMatchObject({ value: 'default', label: 'fade, 1 s', defaultLabel: 'Default (fade, 1 s)', durationSeconds: null })
    expect(transitionLabel({ type: 'none', durationSeconds: 0 }, t)).toBe('No transition')
  })

  test('selects overrides, keeps an override duration and stores typed durations', () => {
    const { clips, program } = timeline()
    const setJoinTransition = vi.fn()
    const [atShort, atLong] = program.joins.map((join) => model(join, clips, setJoinTransition))
    atShort.onSelect('dissolve')
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[0], { type: 'dissolve', durationSeconds: 1 })
    atLong.onSelect('fade')
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[1], { type: 'fade', durationSeconds: 1.5 })
    atLong.onSelect('none')
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[1], { type: 'none', durationSeconds: 0 })
    atLong.onSelect('default')
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[1], null)
    atLong.commitDuration(2.5)
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[1], { type: 'wipeleft', durationSeconds: 2.5 })
    expect(setJoinTransition).toHaveBeenCalledTimes(5)
  })

  test('a new override starts from the default snapped to the frames of the joined pieces', () => {
    const { program } = timeline()
    const setJoinTransition = vi.fn()
    buildJoinTransitionModel({
      join: program.joins[0],
      fps: { num: 25, den: 1 },
      override: null,
      transitionDefaults: { ...defaults, durationSeconds: 1.01 },
      isInvalid: false,
      setJoinTransition,
      t,
    }).onSelect('fade')
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[0], { type: 'fade', durationSeconds: 1 })
  })

  test('flags a transition that does not fit the pieces it joins', () => {
    const { clips, program } = timeline()
    expect(model(program.joins[1], clips, vi.fn(), true).error).toBe('Too long for the video pieces it joins')
  })
})
