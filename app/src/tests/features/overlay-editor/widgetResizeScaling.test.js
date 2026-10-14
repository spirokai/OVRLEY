import { describe, expect, test } from 'vitest'
import {
  buildResizeUpdate,
  buildScaleDraft,
  buildUniformResizeUpdate,
  captureResizeOrigin,
} from '@/features/overlay-editor/utils/widgetResizeScaling'
import {
  buildDragInteractionLayout,
  buildFrameInteractionLayout,
  captureWidgetLayout,
  getLiveWidgetTransform,
} from '@/features/overlay-editor/utils/widgetInteractionGeometry'

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

describe('widgetResizeScaling', () => {
  test('preserves a route rotation while producing a live resize layout', () => {
    const widget = {
      id: 'course-1',
      type: 'course',
      category: 'plots',
      data: { x: 20, y: 30, width: 200, height: 100, rotation: 27 },
    }
    const originLayout = captureWidgetLayout(null, widget, 1)
    const layout = buildFrameInteractionLayout(originLayout, { width: 300, height: 150 })

    expect(layout).toMatchObject({ left: 20, top: 30, width: 300, height: 150, rotation: 27 })
    expect(getLiveWidgetTransform(layout, 1)).toBe('rotate(27deg)')
  })

  test('preserves global scale while dragging an intrinsic metric widget', () => {
    const originLayout = {
      left: 20,
      top: 30,
      width: 100,
      height: 24,
      rotation: 0,
      transform: 'scale(1.5)',
    }
    const layout = buildDragInteractionLayout(originLayout, 12, 8)

    expect(layout).toMatchObject({ left: 32, top: 38, width: 100, height: 24 })
    expect(getLiveWidgetTransform(layout, 1.5)).toBe('scale(1.5)')
  })

  test('owns generic marker scaling outside the resize handler', () => {
    const widget = {
      id: 'course-1',
      type: 'course',
      data: { width: 200, height: 100, marker_size: 10 },
    }
    const update = buildResizeUpdate(captureResizeOrigin(widget), { width: 400, height: 200 }, { round: true })

    expect(update).toMatchObject({ width: 400, height: 200, marker_size: 20 })
  })

  test('builds a complete arc resize update without losing variant data', () => {
    const widget = makeGaugeWidget()
    const origin = captureResizeOrigin(widget)
    const update = buildResizeUpdate(origin, { x: 30, y: 40, width: 440, height: 440 }, { round: true })

    expect(update).toMatchObject({ x: 30, y: 40, width: 440, height: 440, font_size: 120 })
    expect(update.display_variants.arc).toMatchObject({
      width: 440,
      height: 440,
      arc_angle: 225,
      track_thickness: 48,
      track_empty_color: '#222222',
    })
  })

  test('keeps the existing intrinsic-widget scale policy in the shared module', () => {
    const draft = buildScaleDraft(
      {
        font_size: 40,
        icon_size: 20,
        icon_offset_x: 2,
        icon_offset_y: -4,
        triangle_width: 72,
        value_offset: 0,
      },
      1.5,
      { category: 'values', type: 'speed' },
    )

    expect(draft).toEqual({
      font_size: 60,
      icon_size: 30,
      icon_offset_x: 3,
      icon_offset_y: -6,
    })
  })

  test('builds lean-angle Diameter updates without frame dimensions', () => {
    const update = buildUniformResizeUpdate(makeLeanAngleWidget(), 360)

    expect(update.display_variants.lean_angle).toMatchObject({ diameter: 360 })
    expect(update).not.toHaveProperty('width')
    expect(update).not.toHaveProperty('height')
  })

  test('scales every G-force dimension uniformly and preserves configuration fields', () => {
    const widget = makeGForceWidget()
    const origin = captureResizeOrigin(widget)
    const update = buildResizeUpdate(origin, { x: 30, y: 40, width: 330, height: 270 }, { round: true })

    expect(update).toMatchObject({ x: 30, y: 40, width: 330, height: 330 })
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

  test('uses the same G-force policy for intrinsic scale drafts and clamps small dimensions', () => {
    const widget = makeGForceWidget()
    const origin = captureResizeOrigin(widget)
    const draft = buildScaleDraft(origin.data, 1.5, widget)
    const resize = buildResizeUpdate(origin, { width: 330, height: 330 }, { round: true })

    expect(draft).toEqual({
      width: 330,
      height: 330,
      display_variants: {
        g_force: expect.objectContaining({
          width: 330,
          height: 330,
          diameter: 300,
          border_thickness: 3,
          marker_size: 18,
          label_font_size: 21,
          label_offset_x: 6,
          label_offset_y: -9,
        }),
      },
    })
    expect(draft.display_variants.g_force).toMatchObject({
      diameter: resize.display_variants.g_force.diameter,
      border_thickness: resize.display_variants.g_force.border_thickness,
      marker_size: resize.display_variants.g_force.marker_size,
      label_font_size: resize.display_variants.g_force.label_font_size,
      label_offset_x: resize.display_variants.g_force.label_offset_x,
      label_offset_y: resize.display_variants.g_force.label_offset_y,
    })

    const minimums = buildResizeUpdate(origin, { width: 1, height: 1 }, { round: true })
    expect(minimums.display_variants.g_force).toMatchObject({ diameter: 8, border_thickness: 0, marker_size: 1, label_font_size: 8 })

    const tightBorderWidget = makeGForceWidget()
    tightBorderWidget.data.display_variants.g_force.diameter = 9
    tightBorderWidget.data.display_variants.g_force.border_thickness = 4
    const tightBorderOrigin = captureResizeOrigin(tightBorderWidget)
    const tightBorderUpdate = buildResizeUpdate(tightBorderOrigin, { width: 196, height: 196 }, { round: true })
    expect(tightBorderUpdate.display_variants.g_force).toMatchObject({ diameter: 8, border_thickness: 3.5 })
  })

  test('builds a square G-force frame for uniform Size updates', () => {
    const update = buildUniformResizeUpdate(makeGForceWidget(), 360)

    expect(update).toMatchObject({ width: 360, height: 360 })
    expect(update.width).toBe(update.height)
  })
})
