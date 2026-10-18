import { describe, expect, test } from 'vite-plus/test'
import { FitBaseType, FitEncoder } from 'fit-file-parser/encoder'
import parseFitActivityFile from '@/lib/activity/fit-parser'

const field = (number, value, baseType = FitBaseType.Uint32, size = 4) => ({ number, value, baseType, size })
const byte = (number, value) => field(number, value, FitBaseType.Uint8, 1)
const START = 1_100_000_000

function parse(fit) {
  return parseFitActivityFile(new File([fit.close()], 'test.fit'))
}

describe('FIT parsing', () => {
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
