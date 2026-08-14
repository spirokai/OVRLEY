import { describe, expect, test } from 'vitest'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { withAltitudeEditorPresentation } from '@/lib/widget/widget-presentation'

describe('altitude editor presentation', () => {
  test('shows the measured start as a placeholder without enabling calibration', () => {
    const widget = {
      type: 'altitude',
      data: { display_unit: 'ft', starting_altitude: null },
    }

    const presented = withAltitudeEditorPresentation(widget, { elevation: [100.4] })

    expect(presented.data.starting_altitude).toBeNull()
    expect(presented.startingAltitudePlaceholder).toBe(329)
    expect(widget).not.toHaveProperty('startingAltitudePlaceholder')
  })

  test('unit changes preserve absent calibration', () => {
    expect(buildMetricUnitUpdate('altitude', null, 'm', 'ft')).toEqual({
      display_unit: 'ft',
      starting_altitude: null,
    })
  })

  test('unit changes convert configured calibration', () => {
    expect(buildMetricUnitUpdate('altitude', 100, 'm', 'ft')).toEqual({
      display_unit: 'ft',
      starting_altitude: 328,
    })
  })
})
