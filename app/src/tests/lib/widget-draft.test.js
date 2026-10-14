import { describe, expect, test } from 'vitest'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'

describe('widget draft projection', () => {
  test('projects metric display variant geometry into the live canvas widget', () => {
    const widget = {
      id: 'linear-widget',
      category: 'values',
      data: {
        display_type: 'linear',
        width: 300,
        height: 40,
        display_variants: {
          linear: {
            width: 300,
            height: 40,
            track_thickness: 6,
            track_corner_radius: 4,
          },
        },
      },
    }

    const [liveWidget] = applyWidgetDrafts([widget], {
      [widget.id]: {
        display_variants: {
          linear: {
            track_thickness: 18,
            track_corner_radius: 12,
          },
        },
      },
    })

    expect(liveWidget.data.track_thickness).toBe(18)
    expect(liveWidget.data.track_corner_radius).toBe(12)
  })

  test('projects backdrop variant geometry into the live canvas widget', () => {
    const widget = {
      id: 'backdrop-widget',
      category: 'backdrops',
      data: {
        display_type: 'rectangle',
        width: 300,
        height: 150,
        border_thickness: 4,
        display_variants: {
          rectangle: {
            width: 300,
            height: 150,
            corner_radius: 20,
          },
        },
      },
    }

    const [liveWidget] = applyWidgetDrafts([widget], {
      [widget.id]: {
        display_variants: {
          rectangle: {
            corner_radius: 48,
          },
        },
      },
    })

    expect(liveWidget.data.corner_radius).toBe(48)
  })

  test('projects heading tape variant settings into the live canvas widget', () => {
    const widget = {
      id: 'heading-widget',
      category: 'values',
      data: {
        display_type: 'heading_tape',
        width: 400,
        height: 80,
        display_variants: {
          heading_tape: {
            width: 400,
            height: 80,
            pixels_per_degree: 5,
            major_tick_thickness: 2,
            indicator_size: 10,
          },
        },
      },
    }

    const [liveWidget] = applyWidgetDrafts([widget], {
      [widget.id]: {
        display_variants: {
          heading_tape: {
            pixels_per_degree: 9,
            major_tick_thickness: 6,
            indicator_size: 20,
          },
        },
      },
    })

    expect(liveWidget.data.pixels_per_degree).toBe(9)
    expect(liveWidget.data.major_tick_thickness).toBe(6)
    expect(liveWidget.data.indicator_size).toBe(20)
  })
})
