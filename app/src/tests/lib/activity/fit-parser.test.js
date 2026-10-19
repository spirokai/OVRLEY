import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { afterEach, describe, expect, test, vi } from 'vite-plus/test'
import FitParser from 'fit-file-parser'
import { FitBaseType, FitEncoder } from 'fit-file-parser/encoder'
import parseFitActivityFile from '@/lib/activity/fit-parser'

const fixtureDir = path.resolve('../src-tauri/ovrley_core/tests/fixtures/activity')
const field = (number, value, baseType = FitBaseType.Uint32, size = 4) => ({ number, value, baseType, size })
const byte = (number, value) => field(number, value, FitBaseType.Uint8, 1)
const word = (number, value) => field(number, value, FitBaseType.Uint16, 2)
const enumeration = (number, value) => field(number, value, FitBaseType.Enum, 1)
const START = 1_100_000_000

function parse(fit) {
  return parseFitActivityFile(new File([fit.close()], 'test.fit'))
}

describe('FIT parsing', () => {
  test('imports diving units, gas switches, and water temperature from the fixture', async () => {
    const result = await parseFitActivityFile(new File([await readFile(path.join(fixtureDir, 'test-dive.fit'))], 'dive.fit'))
    const samples = result.raw_samples
    expect(samples.map((sample) => sample.depth)).toEqual([1, 5, 10, 15, 20, 12, 2])
    expect(samples[4]).toMatchObject({ first_stop_depth: 3, first_stop_time: 2, time_to_surface: 5, fraction_o2: 0.5, fraction_he: 0 })
    expect(samples[0]).toMatchObject({ fraction_o2: 0.21, temperature: 19, water_temperature: 19 })
    expect(result.options.skip_idle_gap_fill).toBe(true)
  })

  test('uses measured ppO2 and the CCR diluent, preserving missing and sparse readings', async () => {
    const fit = new FitEncoder()
      .writeMessage(12, [enumeration(1, 63)])
      .writeMessage(258, [byte(23, 70), byte(26, 130)])
      .writeMessage(259, [word(254, 0), byte(1, 100), byte(0, 0), enumeration(2, 1), enumeration(3, 0)])
      .writeMessage(259, [word(254, 1), byte(1, 18), byte(0, 45), enumeration(2, 1), enumeration(3, 1)])
      .writeMessage(20, [field(253, START), field(92, 20_000), field(13, 0, FitBaseType.Sint8, 1)])
      .writeMessage(20, [field(253, START + 1), byte(129, 130)])
      .writeMessage(20, [field(253, START + 2)])
    const { raw_samples: samples } = await parse(fit)
    expect(samples[0]).toMatchObject({ average_ppo2: null, fraction_o2: 0.18, fraction_he: 0.45, water_temperature: 0 })
    expect(samples[1].average_ppo2).toBeCloseTo(1.3 / 1.01325, 12)
    expect(samples[2].average_ppo2).toBe(samples[1].average_ppo2)
  })

  test('decodes invalid and absent balance as null while preserving recorded zero', async () => {
    const fit = new FitEncoder()
      .writeMessage(20, [field(253, START), byte(4, 0), byte(30, 0xff)])
      .writeMessage(20, [field(253, START + 1), byte(4, 80)])
      .writeMessage(20, [field(253, START + 2), byte(4, 0), byte(30, 0)])
      .writeMessage(20, [field(253, START + 3), byte(4, 80), byte(30, 50)])
    const { raw_samples: samples, options } = await parse(fit)
    expect(samples.map((sample) => sample.left_right_balance)).toEqual([null, null, 0, 50])
    expect(samples.map((sample) => sample.cadence)).toEqual([0, 80, 0, 80])
    expect(options.skip_idle_gap_fill).toBe(false)
  })
})

const origin = Date.parse('2026-01-01T00:00:00Z')
const timestampAt = (seconds) => new Date(origin + seconds * 1000)

describe('parseFitActivityFile lap timing', () => {
  afterEach(() => vi.restoreAllMocks())

  test('shortens the first lap without shifting records when it starts before the first record', async () => {
    vi.spyOn(FitParser.prototype, 'parseAsync').mockResolvedValue({
      laps: [{ start_time: timestampAt(1) }, { start_time: timestampAt(5.5) }],
      records: [2, 3, 7].map((seconds) => ({ timestamp: timestampAt(seconds), elapsed_time: seconds - 2 })),
    })

    const activity = await parseFitActivityFile({ name: 'laps.fit', arrayBuffer: async () => new ArrayBuffer(0) })

    expect(activity.sync_time).toBe('2026-01-01T00:00:02.000Z')
    expect(activity.lap_markers).toEqual({ BeaconMarkers: [0, 3.5] })
    expect(activity.raw_samples.map((sample) => sample.elapsed_seconds)).toEqual([0, 1, 5])
  })

  test('keeps dive gas switches on the record timeline when a lap starts earlier', async () => {
    vi.spyOn(FitParser.prototype, 'parseAsync').mockResolvedValue({
      laps: [{ start_time: timestampAt(1) }],
      records: [2, 3, 7].map((seconds) => ({ timestamp: timestampAt(seconds), elapsed_time: seconds - 2, depth: 10 })),
      dive_gases: [
        { oxygen_content: 21, helium_content: 0 },
        { oxygen_content: 50, helium_content: 0 },
      ],
      events: [{ event: 'dive_gas_switched', timestamp: timestampAt(4), data: 1 }],
    })

    const activity = await parseFitActivityFile({ name: 'dive-laps.fit', arrayBuffer: async () => new ArrayBuffer(0) })

    expect(activity.raw_samples.map((sample) => sample.elapsed_seconds)).toEqual([0, 1, 2, 5])
    expect(activity.raw_samples[2]).toMatchObject({ timestamp: timestampAt(4), fraction_o2: 0.5, fraction_he: 0 })
  })
})
