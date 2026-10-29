import { DISPLAY_TYPE_DEFINITIONS, WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { describe, expect, test } from 'vite-plus/test'
import { resolveActiveWidgetData, initDisplayVariant, buildFrameGeometryUpdate, mergeWidgetData } from '@/lib/widget/widget-resolver'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'

// ---------------------------------------------------------------------------
// Backdrop resolvers
// ---------------------------------------------------------------------------

describe('backdrop widget resolver', () => {
  test('stores shared backdrop fields at top level and rectangle geometry in the active variant', () => {
    const backdrop = createWidgetDefaults({ type: 'backdrop' })
    const {
      width,
      height,
      corner_radius,
      round_top_left,
      round_top_right,
      round_bottom_left,
      round_bottom_right,
      edge_strips,
      edge_strip_thickness,
      edge_strip_color,
    } = DISPLAY_TYPE_DEFINITIONS.rectangle.variantDefaults

    expect(backdrop).toMatchObject(WIDGET_TYPE_DEFINITIONS.backdrop.defaults)
    expect(backdrop.width).toBeUndefined()
    expect(backdrop.height).toBeUndefined()
    expect(backdrop.display_variants).toEqual({
      rectangle: {
        width,
        height,
        corner_radius,
        round_top_left,
        round_top_right,
        round_bottom_left,
        round_bottom_right,
        edge_strips,
        edge_strip_thickness,
        edge_strip_color,
      },
    })
  })

  test('resolveActiveWidgetData flattens the active rectangle variant for preview rendering', () => {
    const backdrop = createWidgetDefaults({ type: 'backdrop' })
    const resolved = resolveActiveWidgetData(backdrop)

    expect(resolved.width).toBe(DISPLAY_TYPE_DEFINITIONS.rectangle.variantDefaults.width)
    expect(resolved.height).toBe(DISPLAY_TYPE_DEFINITIONS.rectangle.variantDefaults.height)
    expect(resolved.display_type).toBe('rectangle')
    expect(resolved.fill_color).toBe(WIDGET_TYPE_DEFINITIONS.backdrop.defaults.fill_color)
  })

  test('projects live rectangle frame geometry from the active variant', () => {
    const backdrop = createWidgetDefaults({ type: 'backdrop' })
    const patch = buildFrameGeometryUpdate(backdrop, { width: 260, height: 160 })
    const resolved = resolveActiveWidgetData(mergeWidgetData(backdrop, patch))

    expect(resolved.width).toBe(260)
    expect(resolved.height).toBe(160)
    expect(resolved.display_type).toBe('rectangle')
  })

  test('initDisplayVariant preserves existing variants while seeding missing geometry', () => {
    const backdrop = {
      display_type: 'rectangle',
      display_variants: {
        rectangle: { width: 240, height: 140, corner_radius: 8 },
      },
    }

    const result = initDisplayVariant(backdrop, 'circle')

    expect(result.display_variants.rectangle).toEqual(backdrop.display_variants.rectangle)
    expect(result.display_variants.circle).toEqual({ diameter: 200 })
  })

  test('initializes a backdrop variant without overwriting its geometry or inactive variants', () => {
    const backdrop = {
      display_type: 'circle',
      diameter: 200,
      display_variants: {
        rectangle: { width: 240, height: 140, corner_radius: 8 },
        circle: { diameter: 260 },
      },
    }

    const circle = initDisplayVariant(backdrop, 'circle')
    expect(circle.display_variants.circle).toEqual({ diameter: 260 })
    expect(circle.display_variants.rectangle).toBe(backdrop.display_variants.rectangle)
    const rectangle = initDisplayVariant(backdrop, 'rectangle')
    expect(rectangle.display_variants.rectangle).toMatchObject({ width: 240, height: 140, corner_radius: 8 })
    expect(rectangle.display_variants.circle).toBe(backdrop.display_variants.circle)
  })

  test('resolveActiveWidgetData exposes circle diameter as a square frame', () => {
    const backdrop = createWidgetDefaults({ type: 'backdrop', displayType: 'circle' })

    expect(resolveActiveWidgetData(backdrop)).toMatchObject({
      diameter: 200,
      width: 200,
      height: 200,
    })

    const patch = buildFrameGeometryUpdate(backdrop, { width: 240, height: 220 })
    expect(resolveActiveWidgetData(mergeWidgetData(backdrop, patch))).toMatchObject({
      diameter: 240,
      width: 240,
      height: 240,
    })
  })
})

// ---------------------------------------------------------------------------
// Metric resolvers
// ---------------------------------------------------------------------------

describe('resolveActiveWidgetData', () => {
  test('returns flat data as-is for text display_type', () => {
    const data = { value: 'speed', display_type: 'text', font_size: 60, show_icon: true }
    expect(resolveActiveWidgetData(data)).toBe(data)
  })

  test('merges shared fields with heading_tape variant', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      x: 100,
      y: 200,
      font_size: 60,
      color: '#ffffff',
      opacity: 1,
      display_variants: {
        heading_tape: {
          width: 500,
          height: 100,
          rotation: 0,
          pixels_per_degree: 8,
          major_tick_interval: 15,
        },
      },
    }

    const resolved = resolveActiveWidgetData(data)

    expect(resolved.x).toBe(100)
    expect(resolved.y).toBe(200)
    expect(resolved.font_size).toBe(60)
    expect(resolved.color).toBe('#ffffff')
    expect(resolved.width).toBe(500)
    expect(resolved.height).toBe(100)
    expect(resolved.pixels_per_degree).toBe(8)
    expect(resolved.display_type).toBe('heading_tape')
    expect(resolved.id).toBeUndefined()
  })

  test('variant fields override top-level for display-specific settings', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      tick_color: '#000000',
      display_variants: {
        heading_tape: { tick_color: '#ff0000' },
      },
    }

    const resolved = resolveActiveWidgetData(data)
    expect(resolved.tick_color).toBe('#ff0000')
  })

  test('preserves id and display_type from top level', () => {
    const data = {
      id: 'widget-42',
      value: 'heading',
      display_type: 'heading_tape',
      display_variants: {
        heading_tape: { width: 500 },
      },
    }

    const resolved = resolveActiveWidgetData(data)
    expect(resolved.id).toBe('widget-42')
    expect(resolved.display_type).toBe('heading_tape')
  })

  test('resolves lean-angle diameter config without manufacturing frame dimensions', () => {
    const data = {
      value: 'lean_angle',
      display_type: 'lean_angle',
      font_size: 60,
      display_variants: {
        lean_angle: {
          diameter: 180,
          track_thickness: 24,
        },
      },
    }
    const resolved = resolveActiveWidgetData(data)

    expect(resolveActiveWidgetData(resolved)).toEqual(resolved)
    expect(resolved).toMatchObject({ diameter: 180, track_thickness: 24, font_size: 60 })
    expect(resolved).not.toHaveProperty('width')
    expect(resolved).not.toHaveProperty('height')
  })
})

describe('initDisplayVariant', () => {
  test('initializes heading_tape variant from defaults', () => {
    const data = { value: 'heading', display_type: 'heading_tape' }
    const result = initDisplayVariant(data, 'heading_tape')

    expect(result.display_variants.heading_tape).toBeDefined()
    expect(result.display_variants.heading_tape.pixels_per_degree).toBe(DISPLAY_TYPE_DEFINITIONS.heading_tape.variantDefaults.pixels_per_degree)
    expect(result.display_variants.heading_tape.width).toBe(600)
    expect(result.display_variants.heading_tape.height).toBe(100)
  })

  test('seeds variant with current top-level frame geometry', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      width: 600,
      height: 120,
      rotation: 45,
    }
    const result = initDisplayVariant(data, 'heading_tape')

    expect(result.display_variants.heading_tape.width).toBe(600)
    expect(result.display_variants.heading_tape.height).toBe(120)
    expect(result.display_variants.heading_tape.rotation).toBe(45)
  })

  test('preserves existing variant settings while initializing missing defaults', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      display_variants: {
        heading_tape: { pixels_per_degree: 10, label_font: 'Teko.ttf' },
      },
    }
    const result = initDisplayVariant(data, 'heading_tape')

    expect(result.display_variants.heading_tape.pixels_per_degree).toBe(10)
    expect(result.display_variants.heading_tape.label_font).toBe('Teko.ttf')
  })

  test('returns data as-is for text display_type', () => {
    const data = { value: 'heading', display_type: 'text' }
    const result = initDisplayVariant(data, 'text')
    expect(result).toBe(data)
  })

  test('seeds frame geometry for boxed types without non-geometry defaults', () => {
    const data = { value: 'speed', display_type: 'linear' }
    const result = initDisplayVariant(data, 'linear')

    expect(result.display_variants.linear).toBeDefined()
    expect(result.display_variants.linear.width).toBe(200)
    expect(result.display_variants.linear.height).toBe(24)
    expect(result.display_variants.linear.rotation).toBe(0)
    expect(result.display_variants.linear.orientation).toBe('horizontal')
    expect(result.display_variants.linear.track_fill_flat).toBe(false)
  })

  test('backfills missing linear defaults into an existing variant', () => {
    const data = {
      value: 'speed',
      display_type: 'linear',
      display_variants: {
        linear: {
          width: 320,
          height: 90,
          track_corner_radius: 12,
        },
      },
    }
    const result = initDisplayVariant(data, 'linear')

    expect(result.display_variants.linear.width).toBe(320)
    expect(result.display_variants.linear.height).toBe(90)
    expect(result.display_variants.linear.track_corner_radius).toBe(12)
    expect(result.display_variants.linear.orientation).toBe('horizontal')
    expect(result.display_variants.linear.track_fill_flat).toBe(false)
    expect(result.display_variants.linear.show_min_max_labels).toBe(false)
  })

  test('seeds a new G-force variant label font from the value font', () => {
    const result = initDisplayVariant({ value: 'g_force', display_type: 'text', font: 'Roboto.ttf' }, 'g_force')

    expect(result.display_variants.g_force.label_font).toBe('Roboto.ttf')
  })
})

describe('buildFrameGeometryUpdate', () => {
  test('syncs live frame geometry into the active display variant for boxed previews', () => {
    const data = {
      value: 'speed',
      display_type: 'linear',
      display_variants: {
        linear: {
          width: 200,
          height: 60,
          orientation: 'horizontal',
        },
      },
    }

    const patch = buildFrameGeometryUpdate(data, { width: 320, height: 90 })
    const resolved = resolveActiveWidgetData({ ...data, ...patch })

    expect(patch.display_variants.linear.width).toBe(320)
    expect(patch.display_variants.linear.height).toBe(90)
    expect(patch.display_variants.linear.orientation).toBe('horizontal')
    expect(resolved.width).toBe(320)
    expect(resolved.height).toBe(90)
  })

  test('writes rectangle backdrop resize geometry into the active variant', () => {
    const data = createWidgetDefaults({ type: 'backdrop', displayType: 'rectangle' })

    const patch = buildFrameGeometryUpdate(data, { x: 120, y: 140, width: 320, height: 180 })

    expect(patch).toMatchObject({
      x: 120,
      y: 140,
      display_variants: {
        rectangle: {
          width: 320,
          height: 180,
        },
      },
    })
    expect(patch.width).toBeUndefined()
    expect(patch.height).toBeUndefined()
  })

  test('writes circle backdrop resize geometry as diameter', () => {
    const data = createWidgetDefaults({ type: 'backdrop', displayType: 'circle' })

    const patch = buildFrameGeometryUpdate(data, { x: 120, y: 140, width: 260, height: 240 })

    expect(patch.display_variants.circle.diameter).toBe(260)
    expect(patch.width).toBeUndefined()
    expect(patch.height).toBeUndefined()
  })
})
