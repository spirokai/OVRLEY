import { describe, expect, test } from 'vitest'
import { resolveActiveMetricWidgetData, initDisplayVariant, resetCurrentDisplayConfig } from '@/lib/metric-widget-resolver'
import { HEADING_TAPE_DEFAULTS } from '@/lib/standard-widgets'

describe('resolveActiveMetricWidgetData', () => {
  test('returns flat data as-is for text display_type', () => {
    const data = { value: 'speed', display_type: 'text', font_size: 60, show_icon: true }
    expect(resolveActiveMetricWidgetData(data)).toBe(data)
  })

  test('returns data as-is when display_type is missing (defaults to text)', () => {
    const data = { value: 'speed', font_size: 60 }
    expect(resolveActiveMetricWidgetData(data)).toBe(data)
  })

  test('returns null/undefined passthrough', () => {
    expect(resolveActiveMetricWidgetData(null)).toBeNull()
    expect(resolveActiveMetricWidgetData(undefined)).toBeUndefined()
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

    const resolved = resolveActiveMetricWidgetData(data)

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

    const resolved = resolveActiveMetricWidgetData(data)
    expect(resolved.tick_color).toBe('#ff0000')
  })

  test('top-level shared position wins over stale heading_tape variant x/y', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      x: 320,
      y: 180,
      opacity: 0.65,
      display_variants: {
        heading_tape: {
          x: 100,
          y: 100,
          opacity: 1,
          width: 500,
          height: 100,
        },
      },
    }

    const resolved = resolveActiveMetricWidgetData(data)
    expect(resolved.x).toBe(320)
    expect(resolved.y).toBe(180)
    expect(resolved.opacity).toBe(0.65)
    expect(resolved.width).toBe(500)
    expect(resolved.height).toBe(100)
  })

  test('falls back to frame defaults when variant is missing', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      x: 100,
      y: 200,
    }

    const resolved = resolveActiveMetricWidgetData(data)
    // Falls back to manifest defaults (200x60 for heading_tape)
    expect(resolved.width).toBe(200)
    expect(resolved.height).toBe(60)
    expect(resolved.rotation).toBe(0)
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

    const resolved = resolveActiveMetricWidgetData(data)
    expect(resolved.id).toBe('widget-42')
    expect(resolved.display_type).toBe('heading_tape')
  })
})

describe('initDisplayVariant', () => {
  test('initializes heading_tape variant from defaults', () => {
    const data = { value: 'heading', display_type: 'heading_tape' }
    const result = initDisplayVariant(data, 'heading_tape')

    expect(result.display_variants.heading_tape).toBeDefined()
    expect(result.display_variants.heading_tape.pixels_per_degree).toBe(HEADING_TAPE_DEFAULTS.pixels_per_degree)
    expect(result.display_variants.heading_tape.width).toBe(200)
    expect(result.display_variants.heading_tape.height).toBe(60)
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

  test('does not overwrite existing variant', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      display_variants: {
        heading_tape: { pixels_per_degree: 10, custom_field: 'preserved' },
      },
    }
    const result = initDisplayVariant(data, 'heading_tape')

    expect(result.display_variants.heading_tape.pixels_per_degree).toBe(10)
    expect(result.display_variants.heading_tape.custom_field).toBe('preserved')
  })

  test('returns data as-is for text display_type', () => {
    const data = { value: 'heading', display_type: 'text' }
    const result = initDisplayVariant(data, 'text')
    expect(result).toBe(data)
  })

  test('returns data as-is for unknown display_type', () => {
    const data = { value: 'heading', display_type: 'unknown' }
    const result = initDisplayVariant(data, 'unknown')
    expect(result).toBe(data)
  })

  test('seeds frame geometry for boxed types without non-geometry defaults', () => {
    const data = { value: 'speed', display_type: 'linear' }
    const result = initDisplayVariant(data, 'linear')

    expect(result.display_variants.linear).toBeDefined()
    expect(result.display_variants.linear.width).toBe(200)
    expect(result.display_variants.linear.height).toBe(60)
    expect(result.display_variants.linear.rotation).toBe(0)
    expect(result.display_variants.linear.pixels_per_degree).toBeUndefined()
  })

  test('returns data as-is when data is null', () => {
    expect(initDisplayVariant(null, 'heading_tape')).toBeNull()
  })
})

describe('resetCurrentDisplayConfig', () => {
  test('resets text display text-specific fields', () => {
    const data = {
      value: 'speed',
      display_type: 'text',
      show_icon: false,
      icon_size: 100,
      decimals: 3,
    }

    const result = resetCurrentDisplayConfig(data)
    expect(result.show_icon).toBe(true)
    expect(result.icon_size).toBe(45)
    expect(result.decimals).toBe(0)
    expect(result.prefix).toBe('')
    expect(result.suffix).toBe('')
  })

  test('preserves shared fields during text reset', () => {
    const data = {
      value: 'speed',
      display_type: 'text',
      x: 200,
      y: 300,
      font_size: 80,
    }

    const result = resetCurrentDisplayConfig(data)
    expect(result.x).toBe(200)
    expect(result.y).toBe(300)
    expect(result.font_size).toBe(80)
  })

  test('resets heading_tape variant to defaults', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      pixels_per_degree: 15,
      display_variants: {
        heading_tape: { pixels_per_degree: 15, custom: 'value' },
      },
    }

    const result = resetCurrentDisplayConfig(data)
    expect(result.display_variants.heading_tape.pixels_per_degree).toBe(HEADING_TAPE_DEFAULTS.pixels_per_degree)
  })

  test('preserves other display variants during heading_tape reset', () => {
    const data = {
      value: 'heading',
      display_type: 'heading_tape',
      display_variants: {
        heading_tape: { pixels_per_degree: 15 },
        linear: { some_field: 'preserved' },
      },
    }

    const result = resetCurrentDisplayConfig(data)
    expect(result.display_variants.linear.some_field).toBe('preserved')
  })

  test('returns data as-is when data is null', () => {
    expect(resetCurrentDisplayConfig(null)).toBeNull()
  })
})
