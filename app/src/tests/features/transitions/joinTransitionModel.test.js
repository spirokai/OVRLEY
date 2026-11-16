import { describe, expect, test, vi } from 'vite-plus/test'
import { buildJoinTransitionModel, joinOverride } from '@/features/transitions'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, cutEdits, videoStoreState } from '@/tests/helpers/videoFixtures'

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
  Object.assign(
    videoEdits.ride,
    cutEdits([
      { id: 'short', frame: 300, excluded: true },
      { id: 'afterShort', frame: 330 },
      { id: 'long', frame: 900, excluded: true },
      { id: 'afterLong', frame: 1800, transitionIn: { type: 'wipeleft', durationSeconds: 1.5 } },
    ]),
  )
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: defaults })
}

function model(join, clips, setJoinTransition = vi.fn(), isInvalid = false) {
  return buildJoinTransitionModel({ join, fps, override: joinOverride(join, clips), transitionDefaults: defaults, isInvalid, setJoinTransition, t })
}

describe('join transition model', () => {
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
})
