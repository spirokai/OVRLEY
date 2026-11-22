// Writes the Markdown summary of a map label run from its JSON: `node summarize.mjs <run-id>`.
import { existsSync } from 'node:fs'
import { readFile, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const read = async (path) => (existsSync(path) ? JSON.parse(await readFile(path, 'utf8')) : null)
const mean = (values) => values.reduce((sum, value) => sum + value, 0) / values.length
const quantile = (values, q) => {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))]
}
const f = (value, digits = 2) => (value === undefined ? '—' : value.toFixed(digits))
const delta = (off, on) => (off === undefined || on === undefined ? '—' : `${on >= off ? '+' : ''}${(((on - off) / off) * 100).toFixed(0)}%`)
const stat = (values) => (values ? `${f(mean(values))} / ${f(quantile(values, 0.95))}` : '—')

/** Pairs each case's labels-off and labels-on results. */
const pairs = (data) => {
  const cases = new Map()
  for (const result of data?.results ?? []) {
    const entry = cases.get(result.case) ?? {}
    entry[result.labels ? 'on' : 'off'] = result
    cases.set(result.case, entry)
  }
  return cases
}

export async function formatSummary(dir) {
  const [native, environment] = await Promise.all(['native', 'environment'].map((name) => read(join(dir, `${name}.json`))))
  if (!native) throw new Error(`${dir} has no native.json`)
  const nativeCases = pairs(native)
  const lines = ['# Map label benchmark', '']
  if (environment)
    lines.push(
      `${environment.cpu}, ${environment.cores} threads, ${environment.platform}. ${environment.warmup} warm-up + ${environment.frames} measured frames per case.`,
    )
  lines.push(`Native adapter: ${native.adapter}.`)
  lines.push('')

  lines.push('## Frame cost (ms, mean / p95)', '')
  lines.push('`GpuMapRenderer::render` with blocking readback, as export runs it.', '')
  lines.push('| Case | Labels off | Labels on | Δ |')
  lines.push('| --- | --- | --- | --- |')
  for (const [id, { off, on }] of nativeCases) {
    const m = (result) => (result ? mean(result.frames.render_ms) : undefined)
    lines.push(`| ${id} | ${stat(off?.frames.render_ms)} | ${stat(on?.frames.render_ms)} | ${delta(m(off), m(on))} |`)
  }
  lines.push('')

  // A chase frame places labels when its newest placement camera is new: every 0.3 s of activity.
  const plan = await read(join(dir, 'plan.json'))
  const chase = plan?.cases.filter((spec) => spec.camera === 'chase') ?? []
  if (chase.length) {
    lines.push('## Chase frames with labels on: placement frames vs the rest (ms, mean)', '')
    lines.push('| Case | Placement frames | Placement | Other |')
    lines.push('| --- | --- | --- | --- |')
    for (const spec of chase) {
      const placing = spec.frames.slice(plan.warmup).map((frame, index) => {
        const previous = spec.frames[plan.warmup + index - 1]
        return frame.labels.slice(-7, -1).join() !== previous.labels.slice(-7, -1).join()
      })
      const split = (values) => {
        if (!values) return ['—', '—']
        const pick = (want) => mean(values.filter((_, index) => placing[index] === want))
        return [f(pick(true)), f(pick(false))]
      }
      const [np, no] = split(nativeCases.get(spec.id)?.on?.frames.render_ms)
      lines.push(`| ${spec.id} | ${placing.filter(Boolean).length} of ${placing.length} | ${np} | ${no} |`)
    }
    lines.push('')
  }

  lines.push('## Preparation (ms)', '')
  lines.push(
    'Measure: `MapTileSizer::measure` over every case tile, single-threaded (export spreads it over all cores). Cold rasterizes each glyph; warm finds them made. Label layout = warm on − warm off (shaping, line walking, collision boxes); rasterize = cold on − warm on (SDF glyphs). First load is the first frame’s `load_view` (decode, tessellate, lay out, upload). Glyph open is `GlyphService::open` (memory maps the fonts).',
    '',
  )
  lines.push(
    '| Case | Tiles | Measure warm off | Measure warm on | Label layout | Measure cold on | Rasterize | Glyphs | Preload atlas | Glyph open | Renderer new on | First load off | First load on |',
  )
  lines.push('| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |')
  for (const [id, { off, on }] of nativeCases) {
    const p = on.prepare
    lines.push(
      `| ${id} | ${on.tiles} | ${f(off.prepare.measure_warm_ms, 1)} | ${f(p.measure_warm_ms, 1)} | ${f(p.measure_warm_ms - off.prepare.measure_warm_ms, 1)} | ${f(p.measure_cold_ms, 1)} | ${f(p.measure_cold_ms - p.measure_warm_ms, 1)} | ${p.glyphs_rasterized} | ${f(p.preload_glyphs_ms)} | ${f(p.glyph_open_ms)} | ${f(p.renderer_new_ms, 1)} | ${f(off.prepare.first_load_view_ms, 1)} | ${f(p.first_load_view_ms, 1)} |`,
    )
  }
  lines.push('')

  lines.push('## Glyphs (labels on)', '')
  lines.push(
    'Each glyph of the job made again on a fresh `GlyphService`, timed per face (CJK at 2×). Made during frames counts glyphs the service rasterized after preparation: export expects none.',
    '',
  )
  lines.push('| Case | Latin/Greek/Cyrillic/Hebrew glyphs | µs each | CJK glyphs | µs each | Made during frames | Atlas clears |')
  lines.push('| --- | --- | --- | --- | --- | --- | --- |')
  for (const [id, { on }] of nativeCases) {
    const g = on.glyph_faces
    const each = (total, count) => (count ? f((total / count) * 1000, 0) : '—')
    lines.push(
      `| ${id} | ${g.other} | ${each(g.other_ms, g.other)} | ${g.cjk} | ${each(g.cjk_ms, g.cjk)} | ${on.glyphs_made_during_frames} | ${on.atlas_clears} |`,
    )
  }
  lines.push('')
  return lines.join('\n')
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const dir = join(root, 'debug/benchmarks/map-labels', process.argv[2])
  const summary = await formatSummary(dir)
  await writeFile(join(dir, 'summary.md'), summary)
  console.log(summary)
}
