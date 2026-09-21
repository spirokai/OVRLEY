import { describe, expect, test } from 'vitest'

import { parseGpxActivityFile } from '@/lib/activity/gpx-parser'

const exporterGpx = `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="SpringBoot Export" xmlns="http://www.topografix.com/GPX/1/1" xmlns:ext="http://example.com/location/ext">
  <trk>
    <name>Session 4221372</name>
    <trkseg>
      <trkpt lat="47.6662884" lon="21.6567532">
        <ele>194.70001220703125</ele>
        <time>2026-09-20T13:02:09.923Z</time>
        <extensions>
          <ext:speed>0.2771809161487156</ext:speed>
          <ext:speedMps></ext:speedMps>
          <ext:lean>0.0</ext:lean>
          <ext:travelledDistance>0.0</ext:travelledDistance>
          <ext:lapNumber>1</ext:lapNumber>
          <ext:xAccelerationG>0.0074643223</ext:xAccelerationG>
          <ext:yAccelerationG>-0.036097955</ext:yAccelerationG>
          <ext:zAccelerationG>0.07249162</ext:zAccelerationG>
        </extensions>
      </trkpt>
      <trkpt lat="47.66627569158134" lon="21.656767991765975">
        <ele>194.70001220703125</ele>
        <time>2026-09-20T13:02:15.960Z</time>
        <extensions>
          <ext:speedMps>0.22938074038800563</ext:speedMps>
          <ext:lean>-0.25</ext:lean>
          <ext:travelledDistance>1.797462868698045</ext:travelledDistance>
          <ext:lapNumber>2</ext:lapNumber>
          <ext:xAccelerationG>-0.11160793</ext:xAccelerationG>
          <ext:yAccelerationG>-0.0765297</ext:yAccelerationG>
          <ext:zAccelerationG>0.0855134</ext:zAccelerationG>
        </extensions>
      </trkpt>
    </trkseg>
  </trk>
</gpx>`

describe('parseGpxActivityFile', () => {
  test('maps SpringBoot exporter extensions into canonical activity fields', () => {
    const result = parseGpxActivityFile({ name: 'session.gpx' }, exporterGpx)

    expect(result.metadata).toEqual({ activity_name: 'Session 4221372', creator: 'SpringBoot Export' })
    expect(result.raw_samples).toHaveLength(2)
    expect(result.raw_samples[0]).toMatchObject({
      speed: 0.2771809161487156,
      distance: 0,
      lap_number: 1,
      lean_angle: 0,
      g_force_x: 0.0074643223,
      g_force_y: -0.036097955,
      g_force_z: 0.07249162,
    })
    expect(result.raw_samples[1]).toMatchObject({
      speed: 0.22938074038800563,
      distance: 1.797462868698045,
      lap_number: 2,
      lean_angle: -0.25,
      g_force_x: -0.11160793,
      g_force_y: -0.0765297,
      g_force_z: 0.0855134,
    })
  })
})
