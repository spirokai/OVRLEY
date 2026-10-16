import { describe, expect, test, vi } from 'vitest'
import { buildDeletedPartRows, formatSourceTimeInput } from '@/features/toolbar/utils/deletedPartRows'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const t = (key, options) => (typeof options === 'string' ? options : options.defaultValue.replace(/{{(\w+)}}/g, (_, name) => options[name]))
const defaults = { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 2, dialogShown: true }

function rows(deletedParts) {
  const { importedVideos, videoEdits } = videoStoreState([createTestVideo({ id: 'ride', durationSeconds: 120 })])
  videoEdits.ride.deletedParts = deletedParts
  const { clips, program } = buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: defaults })
  const commands = { setDeletedPartRange: vi.fn(), removeDeletedPart: vi.fn(), setJoinTransition: vi.fn(), seek: vi.fn() }
  clips[0].offsetSeconds = 30
  return { commands, program, rows: buildDeletedPartRows({ clip: clips[0], program, transitionDefaults: defaults, commands, t }) }
}

describe('deleted part rows', () => {
  test('formats editable source times to hundredths', () => {
    expect([formatSourceTimeInput(0), formatSourceTimeInput(65.256), formatSourceTimeInput(600)]).toEqual(['0:00.00', '1:05.26', '10:00.00'])
  })

  test('lists parts in source order and commits only ranges with frames in the clip', () => {
    const {
      commands,
      rows: [first, second],
    } = rows([
      { id: 'late', fromSecond: 70, toSecond: 80, transition: null },
      { id: 'early', fromSecond: 10, toSecond: 20, transition: null },
    ])
    expect([first.id, first.fromInput, first.toInput, first.durationLabel, second.id]).toEqual(['early', '0:10.00', '0:20.00', '0:10.00', 'late'])
    first.onToCommit('0:25')
    expect(commands.setDeletedPartRange).toHaveBeenCalledExactlyOnceWith('ride', 'early', 10, 25)
    first.onToCommit('0:05')
    first.onFromCommit('3:00')
    expect(commands.setDeletedPartRange).toHaveBeenCalledOnce()
    second.onRemove()
    expect(commands.removeDeletedPart).toHaveBeenCalledExactlyOnceWith('ride', 'late')
    first.onSeek()
    expect(commands.seek).toHaveBeenCalledExactlyOnceWith(40)
  })

  test('selects the transition of the join at each part', () => {
    const fade = { type: 'wipeleft', durationSeconds: 1.5 }
    const {
      commands,
      program,
      rows: [defaulted, overridden, edge],
    } = rows([
      { id: 'short', fromSecond: 10, toSecond: 11, transition: null },
      { id: 'long', fromSecond: 30, toSecond: 60, transition: fade },
      { id: 'edge', fromSecond: 100, toSecond: 120, transition: null },
    ])
    const join = (id) => program.joins.find((candidate) => candidate.ownerId === id)
    // One second deleted is within the continuity threshold: the default is a hard cut.
    expect(defaulted.transition).toMatchObject({ isAvailable: true, value: 'default', durationInput: null })
    expect(defaulted.transition.options[0].label).toBe('Default (No transition)')
    expect(overridden.transition).toMatchObject({ isAvailable: true, value: 'wipeleft', durationInput: '1.5' })
    expect(overridden.transition.options[0].label).toBe('Default')
    expect(edge.transition.isAvailable).toBe(false)

    defaulted.transition.onTypeChange('dissolve')
    expect(commands.setJoinTransition).toHaveBeenLastCalledWith(join('short'), { type: 'dissolve', durationSeconds: 1 })
    overridden.transition.onTypeChange('fade')
    expect(commands.setJoinTransition).toHaveBeenLastCalledWith(join('long'), { type: 'fade', durationSeconds: 1.5 })
    overridden.transition.onTypeChange('none')
    expect(commands.setJoinTransition).toHaveBeenLastCalledWith(join('long'), { type: 'none', durationSeconds: 0 })
    overridden.transition.onTypeChange('default')
    expect(commands.setJoinTransition).toHaveBeenLastCalledWith(join('long'), null)
    overridden.transition.onDurationCommit('2.5')
    expect(commands.setJoinTransition).toHaveBeenLastCalledWith(join('long'), { type: 'wipeleft', durationSeconds: 2.5 })
    overridden.transition.onDurationCommit('0')
    overridden.transition.onDurationCommit('soon')
    expect(commands.setJoinTransition).toHaveBeenCalledTimes(5)
  })
})
