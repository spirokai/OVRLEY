import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { describe, expect, test } from 'vite-plus/test'
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
