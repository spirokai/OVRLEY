import { buildUniformResizeUpdate } from '@/features/widget-editor/utils/widgetSize'

function makeGaugeWidget(displayType = 'arc') {
  return {
    id: 'arc-1',
    type: 'speed',
    category: 'values',
    data: {
      display_type: displayType,
      font_size: 60,
      display_variants: {
        [displayType]: {
          width: 220,
          height: 220,
          arc_angle: 225,
          track_thickness: 24,
          track_corner_radius: 15,
          track_border_thickness: 2,
          inner_widget_offset_x: 5,
          inner_widget_offset_y: -3,
          min_max_label_font_size: 12,
          track_empty_color: '#222222',
        },
      },
    },
  }
}

function makeLeanAngleWidget() {
  return {
    id: 'lean-angle-1',
    type: 'lean_angle',
    category: 'values',
    data: {
      display_type: 'lean_angle',
      font_size: 60,
      show_units: true,
      show_icon: false,
      prefix: 'L',
      suffix: 'R',
      opacity: 0.75,
      display_variants: {
        lean_angle: {
          diameter: 180,
          track_thickness: 24,
          track_border_thickness: 2,
          value_offset_x: 5,
          value_offset_y: -3,
          track_empty_color: '#222222',
          track_empty_opacity: 0.5,
        },
      },
    },
  }
}

function makeGForceWidget() {
  return {
    id: 'g-force-1',
    type: 'g_force',
    category: 'values',
    data: {
      display_type: 'g_force',
      display_variants: {
        g_force: {
          width: 220,
          height: 220,
          diameter: 200,
          border_thickness: 2,
          marker_size: 12,
          label_font_size: 14,
          label_offset_x: 4,
          label_offset_y: -6,
          fill_color: '#212121',
          fill_opacity: 0.5,
          border_color: '#ffffff',
          border_opacity: 1,
          marker_color: '#ff0000',
          marker_opacity: 0.75,
          label_unit: 'G',
          label_decimals: 1,
          clip_percentile: 99,
          axis_horizontal: 'x',
          axis_vertical: 'y',
          invert_horizontal: false,
          invert_vertical: true,
        },
      },
    },
  }
}

describe('inspector widget size controls', () => {
  test.each(['arc', 'corner'])('preserves %s configuration while scaling editable content', (displayType) => {
    const update = buildUniformResizeUpdate(makeGaugeWidget(displayType), 440)
    expect(update).toMatchObject({ width: 440, height: 440, font_size: 120 })
    expect(update.display_variants[displayType]).toMatchObject({
      width: 440,
      height: 440,
      arc_angle: 225,
      track_thickness: 48,
      track_empty_color: '#222222',
    })
  })

  test('changes lean-angle diameter without persisting derived frame dimensions', () => {
    const update = buildUniformResizeUpdate(makeLeanAngleWidget(), 360)
    expect(update.display_variants.lean_angle).toMatchObject({ diameter: 360, track_thickness: 48, track_border_thickness: 4 })
    expect(update).not.toHaveProperty('width')
    expect(update).not.toHaveProperty('height')
  })

  test('scales every G-force dimension uniformly and preserves configuration fields', () => {
    const widget = makeGForceWidget()
    const update = buildUniformResizeUpdate(widget, 330)

    expect(update).toMatchObject({ width: 330, height: 330 })
    expect(update.display_variants.g_force).toMatchObject({
      width: 330,
      height: 330,
      diameter: 300,
      border_thickness: 3,
      marker_size: 18,
      label_font_size: 21,
      label_offset_x: 6,
      label_offset_y: -9,
      fill_color: '#212121',
      fill_opacity: 0.5,
      border_color: '#ffffff',
      border_opacity: 1,
      marker_color: '#ff0000',
      marker_opacity: 0.75,
      label_unit: 'G',
      label_decimals: 1,
      clip_percentile: 99,
      axis_horizontal: 'x',
      axis_vertical: 'y',
      invert_horizontal: false,
      invert_vertical: true,
    })
    expect(update).not.toHaveProperty('diameter')
    expect(update).not.toHaveProperty('marker_size')
    expect(update).not.toHaveProperty('label_font_size')
  })

  test('keeps G-force geometry within its minimum dimensions', () => {
    const update = buildUniformResizeUpdate(makeGForceWidget(), 1)
    expect(update.display_variants.g_force).toMatchObject({ diameter: 8, border_thickness: 0, marker_size: 1, label_font_size: 8 })
  })
})
