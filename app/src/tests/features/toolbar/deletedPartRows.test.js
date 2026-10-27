import { describe, expect, test, vi } from 'vite-plus/test'
import { buildDeletedPartRows, formatSourceTimeInput } from '@/features/toolbar/utils/deletedPartRows'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const t = (key, options) => (typeof options === 'string' ? options : options.defaultValue.replace(/{{(\w+)}}/g, (_, name) => options[name]))
const defaults = { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 2 }

function rows(deletedParts) {
  const { importedVideos, videoEdits } = videoStoreState([createTestVideo({ id: 'ride', durationSeconds: 120 })])
  videoEdits.ride.deletedParts = deletedParts
  const { clips, program } = buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: defaults })
  const commands = { setDeletedPartRange: vi.fn(), removeDeletedPart: vi.fn(), seek: vi.fn() }
  const joinModels = new Map(program.joins.map((join) => [join.key, { key: join.key }]))
  clips[0].offsetSeconds = 30
  return { commands, rows: buildDeletedPartRows({ clip: clips[0], joinModels, commands, t }) }
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

  test('uses the join control of each part; a part at a clip edge has none', () => {
    const {
      rows: [inner, edge],
    } = rows([
      { id: 'inner', fromSecond: 10, toSecond: 20, transition: null },
      { id: 'edge', fromSecond: 100, toSecond: 120, transition: { type: 'fade', durationSeconds: 1 } },
    ])
    expect(inner.transition).toEqual({ key: 'deletedPart:inner' })
    expect(edge.transition).toMatchObject({ isAvailable: false, value: 'fade', label: 'No join', error: null })
  })
})
