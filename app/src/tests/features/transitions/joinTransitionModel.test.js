import { describe, expect, test, vi } from 'vitest'
import { buildJoinTransitionModel, joinOverride, transitionLabel } from '@/features/transitions'
import { missingTransitionPreviews, transitionPreviewUrl } from '@/features/transitions/utils/transitionPreviews'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const t = (key, options) => (typeof options === 'string' ? options : options.defaultValue.replace(/{{(\w+)}}/g, (_, name) => options[name]))
const defaults = { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 2, dialogShown: true }

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
  return buildJoinTransitionModel({ join, override: joinOverride(join, clips), transitionDefaults: defaults, isInvalid, setJoinTransition, t })
}

describe('join transition model', () => {
  test('every catalogue transition has an animated preview', () => {
    expect(missingTransitionPreviews()).toEqual([])
    expect(transitionPreviewUrl('wipeleft')).toMatch(/wipeleft\.webp/)
    expect(() => transitionPreviewUrl('spin')).toThrow(/no preview/)
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
      durationInput: '1.5',
    })
    expect(intoLater).toMatchObject({ value: 'default', label: 'fade, 1 s', defaultLabel: 'Default (fade, 1 s)', durationInput: null })
    expect(transitionLabel({ type: 'none', durationSeconds: 0 }, t)).toBe('No transition')
  })

  test('selects overrides, keeps an override duration and validates typed durations', () => {
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
    expect(atLong.commitDuration('2.5')).toBeNull()
    expect(setJoinTransition).toHaveBeenLastCalledWith(program.joins[1], { type: 'wipeleft', durationSeconds: 2.5 })
    expect(atLong.commitDuration('0')).toBe('Enter a duration above 0 seconds')
    expect(atLong.commitDuration('soon')).toBe('Enter a duration above 0 seconds')
    expect(setJoinTransition).toHaveBeenCalledTimes(5)
  })

  test('flags a transition that does not fit the pieces it joins', () => {
    const { clips, program } = timeline()
    expect(model(program.joins[1], clips, vi.fn(), true).error).toBe('Too long for the video pieces it joins')
  })
})
