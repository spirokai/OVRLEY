import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { mergeWidgetData, resolveActiveWidgetData } from '@/lib/widget/widget-resolver'
import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import { WIDGET_COLLECTIONS, requireWidgetTypeDefinition } from '@/lib/widget/widget-catalog'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { widgetFonts } from '@/features/wasm-widgets/utils/previewSession'
import i18n from '@/i18n'

const FILL_GRADIENT = {
  track_filled_gradient: true,
  track_filled_gradient_stops: [
    { offset: 0, color: '#40e0d0' },
    { offset: 0.5, color: '#f5d742' },
    { offset: 1, color: '#c65102' },
  ],
}

export const fixtures = {
  ride: ['Evening_Ride-parse-debug.json', 60],
  racechrono: ['sample RaceChrono-parse-debug.json', 60],
  racebox: ['sample Racebox-parse-debug.json', 400],
  dive: ['Shearwater-Cloud-Dive-01-parse-debug.json', 1560],
  // Three 1 km splits are complete at 520 s; the fourth completes at 550 s, inside the measured pass.
  splits: ['Evening_Ride-parse-debug.json', 520],
  // The drone climbs from -5.6 m to 22.4 m in the measured pass, crossing zero and major ticks.
  flight: ['Jul-14th-2026-12-45PM-Flight-Airdata-parse-debug.json', 100],
}

/**
 * The map has no manifest entry until Phase 5, so its effective config is written out here.
 * @param {number} size Output pixels along each edge.
 * @param {number} pitch Camera pitch in degrees.
 * @param {number} scale Scene scale.
 * @returns {object} Effective map widget.
 */
function mapWidget(size, pitch, scale) {
  return {
    id: `map-p${pitch}-${size}`,
    value: 'map',
    x: 200,
    y: 200,
    width: size / scale,
    height: size / scale,
    font: 'Inter.ttf',
    map_style: 'liberty',
    show_roads: true,
    show_paths: true,
    show_water: true,
    show_buildings: true,
    camera_mode: 'follow',
    orientation: 'rider_up',
    pitch,
    range_meters: 400,
    simplify_tolerance_px: 0.5,
    show_full_activity: true,
    completed_line_width: 5,
    completed_line_color: '#ff4d1f',
    completed_line_opacity: 1,
    remaining_line_width: 4,
    remaining_line_color: '#1f7bff',
    remaining_line_opacity: 0.9,
    marker_variant: 'halo',
    marker_variant_diameter: 22,
    marker_size: 6,
    marker_color: '#ff4d1f',
    marker_opacity: 1,
  }
}

/** @param {object} input Activity payloads, HUD template, image path, frame counts, and an optional single case. @returns {object[]} Fixed cases; no rendering occurs. */
export function createCases({ activities, template, image, frames, warmup, fps, only }) {
  const specs = [
    ['course', 'course', 'ride', {}, { width: 560, height: 258, show_full_activity: true }],
    ['elevation', 'elevation', 'ride', {}, { width: 618, height: 163, show_full_activity: true }],
    ['diving_profile', 'diving_profile', 'dive', {}, { width: 618, height: 163, show_full_activity: true }],
    ['rolling-chart', 'rolling_chart', 'ride', {}, {}],
    // Defaults, then the same chart with its area fill switched off.
    ...[
      ['', {}],
      ['-no-area', { area_fill: false }],
      ['-flat-area', { area_fill: true, area_gradient: false }],
    ].flatMap(([suffix, fill]) => [
      [`rolling-chart-1-line${suffix}`, 'rolling_chart', 'ride', {}, { ...fill, series: [{ metric: 'speed', color: '#ffffff' }] }],
      [
        `rolling-chart-5-lines${suffix}`,
        'rolling_chart',
        'ride',
        {},
        {
          ...fill,
          series: ['speed', 'heartrate', 'cadence', 'power', 'temperature'].map((metric, index) => ({
            metric,
            color: ['#ffffff', '#e0103a', '#30c4d1', '#f5b700', '#7ed957'][index],
          })),
        },
      ],
    ]),
    [
      'rolling-chart-area-gradient',
      'rolling_chart',
      'ride',
      {},
      { width: 580, height: 110, window_seconds: 30, area_fill: true, area_gradient: true },
    ],
    ['gradient', 'gradient', 'ride', {}, { font_size: 45, triangle_width: 36 }],
    ['time-daytime', 'time', 'ride', {}, { font_size: 45, time_mode: 'daytime' }],
    ['time-elapsed', 'time', 'ride', {}, { font_size: 45, time_mode: 'elapsed', elapsed_origin: 'activity' }],
    ['label', 'label', 'ride', {}, { font_size: 45, text: 'OVRLEY / AFTERNOON RIDE' }],
    ['backdrop-rectangle', 'backdrop', 'ride', { displayType: 'rectangle' }, { display_variants: { rectangle: { width: 410, height: 53 } } }],
    ['backdrop-circle', 'backdrop', 'ride', { displayType: 'circle' }, { display_variants: { circle: { diameter: 232 } } }],
    ['raster', 'raster', 'ride', {}, { path: image, width: 256, height: 256 }],
    ['speed-text', 'speed', 'ride', { displayType: 'text' }, { font_size: 45, show_units: true, display_unit: 'kmh' }],
    ['heading-tape', 'heading', 'ride', { displayType: 'heading_tape' }, { display_variants: { heading_tape: { width: 600, height: 100 } } }],
    ...['indicator', 'compass'].map((rotation_mode) => [
      `heading-compass-${rotation_mode}`,
      'heading',
      'ride',
      { displayType: 'heading_compass' },
      { display_variants: { heading_compass: { diameter: 200, rotation_mode } } },
    ]),
    ...['line', 'arrow'].map((indicator_style) => [
      `heart-zones-${indicator_style}`,
      'heartrate',
      'ride',
      { displayType: 'heart_zones' },
      { display_variants: { heart_zones: { width: 420, height: 36, indicator_style } } },
    ]),
    ...[
      ['left', true],
      ['right', false],
    ].map(([tape_orientation, show_vertical_speed]) => [
      `altitude-tape-${tape_orientation}`,
      'altitude',
      'flight',
      { displayType: 'altitude_tape' },
      { display_variants: { altitude_tape: { width: 110, height: 420, tape_orientation, show_vertical_speed } } },
    ]),
    [
      'lean-angle',
      'lean_angle',
      'racechrono',
      { displayType: 'lean_angle' },
      { font_size: 45, display_variants: { lean_angle: { diameter: 232, track_thickness: 65 } } },
    ],
    [
      'lean-angle-gradient',
      'lean_angle',
      'racechrono',
      { displayType: 'lean_angle' },
      { font_size: 45, display_variants: { lean_angle: { diameter: 232, track_thickness: 65, ...FILL_GRADIENT } } },
    ],
    [
      'g-force',
      'g_force',
      'racebox',
      { displayType: 'g_force' },
      { display_variants: { g_force: { diameter: 232, width: 256, height: 256, marker_size: 12, label_font_size: 35 } } },
    ],
  ]
  for (const [display, gradient] of [
    ['linear', false],
    ['linear', true],
    ['arc', false],
    ['arc', true],
    ['corner', false],
    ['gt_gauge', false],
  ])
    for (const style of display === 'gt_gauge' ? ['fill'] : ['fill', 'bars']) {
      specs.push([
        `speed-${display}-${style}${gradient ? '-gradient' : ''}`,
        'speed',
        'ride',
        { displayType: display },
        {
          show_units: true,
          display_unit: 'kmh',
          display_variants: {
            [display]: {
              width: 232,
              height: display === 'linear' ? 40 : 232,
              track_fill_style: style,
              ...(style === 'bars' ? { bar_count: 20, bar_gap: 3 } : {}),
              ...(gradient ? FILL_GRADIENT : {}),
            },
          },
        },
      ])
    }
  specs.push([
    'speed-linear-fill-gradient-flat',
    'speed',
    'ride',
    { displayType: 'linear' },
    {
      show_units: true,
      display_unit: 'kmh',
      display_variants: { linear: { width: 232, height: 40, track_fill_style: 'fill', track_fill_flat: true, ...FILL_GRADIENT } },
    },
  ])
  for (const mode of ['current_lap', 'best_lap', 'delta', 'lap_log'])
    specs.push([`lap-${mode}`, 'lap_timer', 'racebox', { displayType: 'lap_timer', lapTimerMode: mode }, {}])
  specs.push(['splits-run', 'splits_run', 'splits', { displayType: 'splits_run' }, {}])
  for (const size of [500, 1000]) for (const pitch of [0, 40, 60]) specs.push([`map-p${pitch}-${size}`, 'map', 'ride', {}, { size, pitch }])
  const globals = { ...DEFAULT_GLOBAL_DEFAULTS, ...template.settings.globalDefaults }
  const result = []
  const selected = [['empty', null, 'ride', {}, {}], ...specs].filter(([id]) => only === undefined || id === 'empty' || id === only)
  if (only !== undefined && selected.length === 1) throw new Error(`Unknown case: ${only}`)
  for (const [id, type, activity, selection, overrides] of selected) {
    const data = activities[activity]
    if (data === undefined) throw new Error(`${id}: fixture ${fixtures[activity][0]} is missing from debug/activities`)
    const start_frame = fixtures[activity][1] * fps
    const scene = {
      ...template.config.scene,
      fps,
      updateRate: 1,
      start: start_frame / fps,
      end: (start_frame + warmup + frames) / fps,
      custom_export_range_active: true,
      preview_duration_seconds: data.trim_end_seconds,
      export_start_seconds: start_frame / fps,
    }
    if (scene.end > data.trim_end_seconds) throw new Error(`${id}: playback exceeds activity duration`)
    const required = {
      speed: ['speed'],
      heading: ['heading'],
      lean_angle: ['lean_angle'],
      g_force: ['g_force_x', 'g_force_y'],
      course: ['sample_course_points'],
      elevation: ['sample_elevations'],
      diving_profile: ['depth', 'first_stop_depth'],
      rolling_chart: ['speed', 'heartrate'],
      gradient: ['gradient'],
      lap_timer: ['lap_time_seconds', 'delta_to_best_lap_seconds'],
      splits_run: ['distance'],
      map: ['sample_course_points'],
    }
    for (const field of required[type] ?? [])
      if (
        !data[field]?.some(
          (value, index) => value !== null && data.sample_elapsed_seconds[index] >= scene.start && data.sample_elapsed_seconds[index] < scene.end,
        )
      )
        throw new Error(`${id}: no ${field} telemetry in playback window`)
    const config = { scene, ...Object.fromEntries(Object.keys(WIDGET_COLLECTIONS).map((key) => [key, []])) }
    let dynamic = false
    if (type === 'map') dynamic = true
    else if (type !== null) {
      const definition = requireWidgetTypeDefinition(type)
      const widget = createWidgetDefaults({ type, ...selection }, globals, { coursePoints: data.sample_course_points, sceneFontSize: 19.5 })
      config[definition.collection].push(mergeWidgetData(widget, { ...overrides, id, x: 200, y: 200 }))
      dynamic = definition.timeDependent
    }
    const effective = createEditorEffectiveConfig({ config, globalDefaults: globals, language: i18n.resolvedLanguage })
    let request = null
    let fonts = []
    if (type === 'map') {
      request = { type, widget: mapWidget(overrides.size, overrides.pitch, effective.scene.scale), scene: effective.scene }
      fonts = widgetFonts({ type, data: request.widget })
    } else if (type !== null) {
      const widget = resolveActiveWidgetData(effective[requireWidgetTypeDefinition(type).collection][0])
      request = { type, widget: type === 'raster' ? { ...widget, resourceId: 'benchmark-image' } : widget, scene: effective.scene }
      fonts = widgetFonts({ type, data: request.widget })
    }
    // The native resource step derives the table core would hand the editor for this split length.
    const split_table = type === 'splits_run' ? `split-table-${id}.json` : null
    // WASM has no GPU renderer: both preview backends receive one finished frame of this edge length.
    const map_frame = type === 'map' ? overrides.size : null
    result.push({ id, activity, start_frame, dynamic, config: effective, request, fonts, split_table, map_frame })
  }
  return result
}

const WASM_BASE_URL = '/wasm-bindings'
let modulePromise
/** @param {object} input One fixed case, protocol, and prepared fonts. @returns {Promise<object>} Playback samples and stage totals. */
export async function play({ entry, frames, warmup, fps, fonts }) {
  modulePromise ??= import(/* @vite-ignore */ `${WASM_BASE_URL}/wasm_bindings.js`).then(({ default: create }) =>
    create({ locateFile: (name) => `${WASM_BASE_URL}/${name}` }),
  )
  const module = await modulePromise
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  const session = module._ovrley_preview_create()
  const read = (ptr, length) => decoder.decode(module.HEAPU8.subarray(ptr, ptr + length))
  const fail = () => {
    throw new Error(read(module._ovrley_preview_error_ptr(session), module._ovrley_preview_error_len(session)))
  }
  const check = (status) => {
    if (status !== 0) fail()
  }
  const upload = (bytes, call) => {
    const ptr = module._ovrley_alloc(bytes.length)
    try {
      module.HEAPU8.set(bytes, ptr)
      return call(ptr, bytes.length)
    } finally {
      module._ovrley_dealloc(ptr, bytes.length)
    }
  }
  const send = (value, call) => upload(encoder.encode(JSON.stringify(value)), call)
  const bytes = async (path) => {
    const response = await fetch(`/data/${path}`)
    if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`)
    return new Uint8Array(await response.arrayBuffer())
  }
  const canvas = document.querySelector('canvas')
  const context = canvas.getContext('2d')
  const stages = Object.fromEntries(['wasm.render', 'geometry', 'pixels.copy', 'canvas.upload'].map((name) => [name, { count: 0, total_ms: 0 }]))
  let lastRevision = -1,
    image,
    geometry = null,
    redraws = 0
  try {
    for (const font of fonts) {
      const data = await bytes(font.path)
      send(font.request, (id, idLen) => upload(data, (ptr, len) => check(module._ovrley_preview_font(session, id, idLen, ptr, len))))
    }
    if (entry.dynamic) upload(await bytes(`${entry.activity}.json`), (ptr, len) => check(module._ovrley_preview_activity(session, ptr, len)))
    if (entry.split_table !== null) upload(await bytes(entry.split_table), (ptr, len) => check(module._ovrley_preview_split_table(session, ptr, len)))
    if (entry.request?.type === 'raster') {
      const png = await bytes('image.png')
      upload(encoder.encode('benchmark-image'), (id, idLen) =>
        upload(png, (ptr, len) => check(module._ovrley_preview_image(session, id, idLen, ptr, len))),
      )
    }
    if (entry.map_frame !== null) {
      const pixels = new Uint8Array(entry.map_frame * entry.map_frame * 4).fill(255)
      upload(pixels, (ptr, len) => check(module._ovrley_preview_map_frame(session, entry.map_frame, entry.map_frame, ptr, len)))
    }
    const widget = entry.request === null ? 0 : send(entry.request, (ptr, len) => module._ovrley_preview_widget_create(session, ptr, len))
    if (entry.request !== null && widget === 0) fail()
    const step = (index, measured) => {
      if (widget === 0 || (!entry.dynamic && lastRevision !== -1)) return false
      let mark = performance.now()
      const stage = (name) => {
        const now = performance.now()
        if (measured) {
          stages[name].count++
          stages[name].total_ms += now - mark
        }
        mark = now
      }
      if (entry.dynamic) check(module._ovrley_preview_render(session, (entry.start_frame + index) / fps))
      stage('wasm.render')
      const revision = module._ovrley_preview_revision(session, widget)
      if (revision === lastRevision) return false
      geometry = JSON.parse(read(module._ovrley_preview_geometry_ptr(session, widget), module._ovrley_preview_geometry_len(session, widget)))
      if (!image || image.width !== geometry.width || image.height !== geometry.height) {
        canvas.width = geometry.width
        canvas.height = geometry.height
        image = new ImageData(geometry.width, geometry.height)
      }
      stage('geometry')
      const ptr = module._ovrley_preview_pixels(session, widget)
      image.data.set(module.HEAPU8.subarray(ptr, ptr + image.data.length))
      stage('pixels.copy')
      context.putImageData(image, 0, 0)
      stage('canvas.upload')
      lastRevision = revision
      return true
    }
    step(0, false)
    for (let i = 0; i < warmup; i++) step(i, false)
    const ms = new Array(frames)
    const redraw_ms = []
    for (let i = 0; i < frames; i++) {
      const started = performance.now()
      const changed = step(warmup + i, true)
      ms[i] = performance.now() - started
      if (changed) {
        redraws++
        redraw_ms.push(ms[i])
      }
      if (i % 50 === 49) await new Promise((resolve) => setTimeout(resolve, 0))
    }
    return { id: entry.id, backend: 'wasm', ms, redraw_ms, redraws, stages, raster: geometry === null ? null : [geometry.width, geometry.height] }
  } finally {
    module._ovrley_preview_destroy(session)
  }
}
