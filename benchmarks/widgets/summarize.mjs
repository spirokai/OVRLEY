import { readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const mean = ({ ms }) => ms.reduce((sum, value) => sum + value, 0) / ms.length
const renderOperation = { native: 'frame.draw', 'native-preview': 'preview.render', wasm: 'wasm.render' }
const drawMean = ({ backend, stages, ms }) => stages[renderOperation[backend]].total_ms / ms.length

/** @param {object[]} results Native and/or WASM playback results. @returns {string} Markdown averages and operation timings. */
export function formatSummary(results) {
  if (!Array.isArray(results) || !results.length) throw new Error('Expected a non-empty array of benchmark results')
  const widgets = new Map()
  for (const row of results) {
    if (
      !row ||
      typeof row.id !== 'string' ||
      !row.id.length ||
      !Object.hasOwn(renderOperation, row.backend) ||
      !Array.isArray(row.ms) ||
      !row.ms.length ||
      !row.ms.every((value) => Number.isFinite(value) && value >= 0) ||
      !row.stages ||
      typeof row.stages !== 'object' ||
      Array.isArray(row.stages)
    )
      throw new Error('Invalid benchmark result: expected id, backend, non-empty ms samples, and stages')
    for (const [name, stage] of Object.entries(row.stages)) {
      if (
        !stage ||
        !Number.isSafeInteger(stage.count) ||
        stage.count < 0 ||
        !Number.isFinite(stage.total_ms) ||
        stage.total_ms < 0 ||
        (stage.count === 0 && stage.total_ms !== 0)
      )
        throw new Error(`${row.backend}/${row.id}/${name}: invalid operation count or total_ms`)
    }
    const operation = renderOperation[row.backend]
    if (!Object.hasOwn(row.stages, operation)) throw new Error(`${row.backend}/${row.id}: missing ${operation} timings`)
    if (!widgets.has(row.id)) widgets.set(row.id, {})
    const widget = widgets.get(row.id)
    if (Object.hasOwn(widget, row.backend)) throw new Error(`Duplicate result: ${row.backend}/${row.id}`)
    widget[row.backend] = row
  }
  const ranked = [...widgets].sort((a, b) => Math.max(...Object.values(b[1]).map(drawMean)) - Math.max(...Object.values(a[1]).map(drawMean)))
  const lines = [
    '| Widget | Native preview ms/frame | WASM preview ms/frame | WASM / native preview |',
    '| --- | ---: | ---: | ---: |',
    ...ranked.map(
      ([id, rows]) =>
        `| ${id} | ${rows['native-preview'] ? drawMean(rows['native-preview']).toFixed(4) : '—'} | ${rows.wasm ? drawMean(rows.wasm).toFixed(4) : '—'} | ${rows['native-preview'] && rows.wasm && drawMean(rows['native-preview']) > 0 ? `${(drawMean(rows.wasm) / drawMean(rows['native-preview'])).toFixed(2)}x` : '—'} |`,
    ),
    '',
    'Native preview and WASM preview run the same `PreviewSession` with the same fonts, activity, frame times, widget-sized premultiplied surface, unchanged-value skips, RGBA readback, alpha conversion, and geometry serialization. Both exclude setup, JavaScript pixel copying, and canvas upload. WASM additionally crosses the browser ABI; native preview calls the session directly.',
    '',
    '| Widget | Native export draw ms/frame | Native export total ms/frame | WASM playback ms/frame |',
    '| --- | ---: | ---: | ---: |',
    ...ranked.map(
      ([id, rows]) =>
        `| ${id} | ${rows.native ? drawMean(rows.native).toFixed(4) : '—'} | ${rows.native ? mean(rows.native).toFixed(4) : '—'} | ${rows.wasm ? mean(rows.wasm).toFixed(4) : '—'} |`,
    ),
    '',
    'These workflow timings have different scopes. Native export draw uses `frame.draw`, excluding full-scene restoration and surface wrapping; its total includes those operations. WASM playback also includes geometry handling, pixel copying, and canvas upload. Do not interpret export draw versus WASM preview as a like-for-like backend ratio.',
    '',
    'Operation averages use all measured playback frames, including frames that skipped a redraw. Avg ms/call uses the operation count.',
    '',
    '| Widget | Backend | Operation | Frames | Calls | Avg ms/frame | Avg ms/call |',
    '| --- | --- | --- | ---: | ---: | ---: | ---: |',
  ]
  for (const [, rows] of ranked) {
    for (const row of Object.values(rows)) {
      for (const [name, stage] of Object.entries(row.stages)) {
        lines.push(
          `| ${row.id} | ${row.backend} | ${name} | ${row.ms.length} | ${stage.count} | ${(stage.total_ms / row.ms.length).toFixed(4)} | ${stage.count ? (stage.total_ms / stage.count).toFixed(4) : '—'} |`,
        )
      }
    }
  }
  lines.push('', '`frame.draw` includes its child widget operations; do not add it to those child timings.', '')
  return lines.join('\n')
}

async function main() {
  const [id, ...extra] = process.argv.slice(2)
  if (!/^\d+-\d+$/.test(id) || extra.length) throw new Error('Usage: pnpm benchmark:widgets:summary <run-id>')
  const directory = resolve(dirname(fileURLToPath(import.meta.url)), '../../debug/benchmarks/widgets', id)
  const names = await readdir(directory)
  const files = ['native.json', 'native-preview.json', 'wasm.json'].filter((name) => names.includes(name))
  if (!files.length) throw new Error(`${id}: no benchmark result files found`)
  const results = (await Promise.all(files.map(async (name) => JSON.parse(await readFile(join(directory, name), 'utf8'))))).flat()
  const output = join(directory, 'summary.md')
  await writeFile(output, formatSummary(results))
  console.info(`Summary: ${output}`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
}
