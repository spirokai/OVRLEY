import { describe, expect, test, vi } from 'vite-plus/test'
import { buildVideoSegmentRows } from '@/features/toolbar/utils/videoSegmentRows'
import { createTestVideo, cutEdits } from '@/tests/helpers/videoFixtures'

function rowsOf(joinModels = new Map()) {
  const clip = {
    ...createTestVideo({ id: 'ride', durationSeconds: 90 }),
    offsetSeconds: 10,
    ...cutEdits([
      { id: 'c1', frame: 900, excluded: true },
      { id: 'c2', frame: 1800 },
    ]),
  }
  const commands = { setVideoSegmentExcluded: vi.fn(), requestCutRemoval: vi.fn(), seek: vi.fn() }
  return { rows: buildVideoSegmentRows({ clip, joinModels, commands }), commands }
}

describe('buildVideoSegmentRows', () => {
  test('lists parts in source order with their range and export state', () => {
    const { rows } = rowsOf()
    expect(rows.map(({ key, startLabel, endLabel, included }) => [key, startLabel, endLabel, included])).toEqual([
      ['start', '00:00', '00:30', true],
      ['c1', '00:30', '01:00', false],
      ['c2', '01:00', '01:30', true],
    ])
  })

  test('only a part with a join leading into it has a transition control', () => {
    const control = { value: 'default' }
    const { rows } = rowsOf(new Map([['ride:c2', control]]))
    expect(rows.map((row) => row.transition)).toEqual([null, null, control])
  })

  test('commands act on their own part; the first part starts at no cut', () => {
    const { rows, commands } = rowsOf()
    rows[1].onIncludedChange(true)
    expect(commands.setVideoSegmentExcluded).toHaveBeenLastCalledWith('ride', 'c1', false)
    rows[2].onIncludedChange(false)
    expect(commands.setVideoSegmentExcluded).toHaveBeenLastCalledWith('ride', 'c2', true)
    rows[1].onSeek()
    expect(commands.seek).toHaveBeenLastCalledWith(40)
    rows[2].onRemoveCut()
    expect(commands.requestCutRemoval).toHaveBeenLastCalledWith('ride', 'c2')
    expect(rows[0].onRemoveCut).toBeNull()
  })
})
