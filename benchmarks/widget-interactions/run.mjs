import { parseArgs, promisify } from 'node:util'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { cpus, platform } from 'node:os'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { createServer } from 'vite-plus'
import { chromium } from '@playwright/test'
import { artifactDir } from '../../scripts/build-wasm.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const appRequire = createRequire(join(root, 'app/package.json'))
const execute = promisify(execFile)
const json = async (path) => JSON.parse(await readFile(path, 'utf8'))
const save = (path, value) => writeFile(path, JSON.stringify(value, null, 2) + '\n')
const { values } = parseArgs({
  options: {
    widgets: { type: 'string', default: '15' },
    frames: { type: 'string', default: '180' },
    warmup: { type: 'string', default: '30' },
    burst: { type: 'string', default: '1' },
    workload: { type: 'string', default: 'all' },
    operation: { type: 'string', default: 'all' },
    label: { type: 'string', default: 'baseline' },
    compare: { type: 'string' },
    'rect-stacks': { type: 'boolean', default: false },
  },
})
const protocol = {}
for (const key of ['widgets', 'frames', 'warmup', 'burst']) {
  const value = Number(values[key])
  if (!/^\d+$/.test(values[key]) || !Number.isSafeInteger(value) || value < (key === 'warmup' ? 0 : key === 'widgets' ? 2 : 1))
    throw new Error(`${key} must be an integer >= ${key === 'warmup' ? 0 : key === 'widgets' ? 2 : 1}`)
  protocol[key] = value
}
if (!Number.isSafeInteger(protocol.frames * protocol.burst) || !Number.isSafeInteger(protocol.warmup * protocol.burst))
  throw new Error('Frame count multiplied by burst must be a safe integer')
if (!['all', 'text', 'mixed'].includes(values.workload) || !['all', 'drag', 'resize'].includes(values.operation))
  throw new Error('Choose workload all/text/mixed and operation all/drag/resize')
if (!/^[a-zA-Z0-9_-]+$/.test(values.label)) throw new Error('label must contain only letters, numbers, underscores, or hyphens')
if (values['rect-stacks'] && values.compare !== undefined)
  throw new Error('Rectangle stack capture adds diagnostic overhead; run it without --compare')
protocol.workloads = values.workload === 'all' ? ['text', 'mixed'] : [values.workload]
protocol.operations = values.operation === 'all' ? ['drag', 'resize'] : [values.operation]

function stats(samples) {
  const sorted = [...samples].sort((a, b) => a - b)
  const total = samples.reduce((sum, value) => sum + value, 0)
  return {
    calls: samples.length,
    total_ms: total,
    avg_ms: total / samples.length,
    p95_ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
    max_ms: sorted.at(-1),
  }
}

function delta(before, after) {
  return before === 0 ? '—' : `${((after / before - 1) * 100).toFixed(1)}%`
}

function summary(results, baseline) {
  const lines = [
    `# Widget interactions: ${values.label}`,
    '',
    `${protocol.widgets} selected widgets; ${protocol.frames} measured input frames; ${protocol.warmup} warm-up frames; ${protocol.burst} mouse moves per frame. Playhead paused at 60 seconds.`,
    '',
    ...(values['rect-stacks']
      ? ['Rectangle stack capture is enabled. Timings include diagnostic overhead and are not comparable to normal runs.', '']
      : []),
    '| Case | First input → group geometry ms | Frame interval avg ms | p95 ms | Max ms | Intervals >25 ms | Config updates | Canvas uploads | Mouseup CPU ms |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |',
  ]
  for (const row of results) {
    const frame = stats(row.frame_intervals_ms)
    lines.push(
      `| ${row.id} | ${row.first_presentation_ms.toFixed(2)} | ${frame.avg_ms.toFixed(2)} | ${frame.p95_ms.toFixed(2)} | ${frame.max_ms.toFixed(2)} | ${row.frame_intervals_ms.filter((ms) => ms > 25).length} | ${row.stages['wasm.config_update']?.length ?? 0} | ${row.stages['canvas.upload']?.length ?? 0} | ${row.release_ms.toFixed(2)} |`,
    )
  }
  lines.push(
    '',
    'Frame intervals are observed browser refresh intervals, including rendering and browser work. The initial response probe waits for every selected widget to present changed geometry and runs separately from steady-state timing.',
    '',
    '| Case | Stage | Calls | Total ms | ms/input frame | p95 ms/call | Max ms/call |',
    '| --- | --- | ---: | ---: | ---: | ---: | ---: |',
  )
  for (const row of results) {
    for (const [name, samples] of Object.entries(row.stages).sort(([a], [b]) => a.localeCompare(b))) {
      const stage = stats(samples)
      lines.push(
        `| ${row.id} | ${name} | ${stage.calls} | ${stage.total_ms.toFixed(2)} | ${(stage.total_ms / protocol.frames).toFixed(3)} | ${stage.p95_ms.toFixed(3)} | ${stage.max_ms.toFixed(3)} |`,
      )
    }
    lines.push(`| ${row.id} | canvas.upload MiB | — | ${(row.counters['canvas.upload.bytes'] / 1048576).toFixed(2)} MiB | — | — | — |`)
  }
  lines.push(
    '',
    '`raf.render` is the production preview callback; `raf.bound publishDrafts` is draft publication. Callback names may be renamed by the development transformer. Stage timers overlap: WASM, serialization, and canvas work are inside the preview callback; DOM measurements are also inside Moveable. Do not add nested stages. Canvas upload measures API submission, and geometry presentation does not measure physical display latency.',
    '',
  )
  if (baseline) {
    lines.push(
      `Compared with **${baseline.label}**. Negative deltas mean less time.`,
      '',
      '| Case | Measurement | Baseline ms | Current ms | Change |',
      '| --- | --- | ---: | ---: | ---: |',
    )
    for (const row of results) {
      const before = baseline.results.find((entry) => entry.id === row.id)
      for (const [name, oldMs, newMs] of [
        ['First input → group geometry', before.first_presentation_ms, row.first_presentation_ms],
        ['Frame interval p95', stats(before.frame_intervals_ms).p95_ms, stats(row.frame_intervals_ms).p95_ms],
        ['Mouseup CPU', before.release_ms, row.release_ms],
      ])
        lines.push(`| ${row.id} | ${name} | ${oldMs.toFixed(2)} | ${newMs.toFixed(2)} | ${delta(oldMs, newMs)} |`)
    }
    lines.push('', '| Case | Stage | Baseline ms/input frame | Current ms/input frame | Change |', '| --- | --- | ---: | ---: | ---: |')
    for (const row of results) {
      const before = baseline.results.find((entry) => entry.id === row.id)
      for (const name of new Set([...Object.keys(before.stages), ...Object.keys(row.stages)])) {
        // A stage with no calls is absent, including when an optimization removes it entirely.
        const oldMs = (before.stages[name] ?? []).reduce((sum, ms) => sum + ms, 0) / protocol.frames
        const newMs = (row.stages[name] ?? []).reduce((sum, ms) => sum + ms, 0) / protocol.frames
        lines.push(`| ${row.id} | ${name} | ${oldMs.toFixed(3)} | ${newMs.toFixed(3)} | ${delta(oldMs, newMs)} |`)
      }
    }
  }
  return lines.join('\n') + '\n'
}

async function main() {
  const baseline = values.compare === undefined ? null : await json(resolve(root, values.compare))
  if (values.compare !== undefined) {
    if (
      baseline === null ||
      baseline.schema !== 1 ||
      typeof baseline.label !== 'string' ||
      JSON.stringify(baseline.protocol) !== JSON.stringify(protocol) ||
      !Array.isArray(baseline.results)
    )
      throw new Error('Baseline schema and full frame/workload protocol must match this run')
    const expected = new Set(protocol.workloads.flatMap((workload) => protocol.operations.map((operation) => `${workload}-${operation}`)))
    for (const row of baseline.results) {
      if (!expected.delete(row.id) || row.stages === null || typeof row.stages !== 'object' || Array.isArray(row.stages))
        throw new Error('Invalid or duplicate baseline case')
      if (
        !Number.isFinite(row.first_presentation_ms) ||
        row.first_presentation_ms < 0 ||
        !Number.isFinite(row.release_ms) ||
        row.release_ms < 0 ||
        !Array.isArray(row.frame_intervals_ms) ||
        row.frame_intervals_ms.length !== protocol.frames ||
        !row.frame_intervals_ms.every((ms) => Number.isFinite(ms) && ms >= 0)
      )
        throw new Error(`Invalid baseline frame measurements for ${row.id}`)
      for (const samples of Object.values(row.stages)) {
        if (!Array.isArray(samples) || samples.length === 0 || !samples.every((ms) => Number.isFinite(ms) && ms >= 0))
          throw new Error(`Invalid baseline stage samples for ${row.id}`)
      }
    }
    if (expected.size !== 0) throw new Error(`Baseline is missing cases: ${[...expected].join(', ')}`)
  }
  // All rendering artifacts must already exist. This runner never invokes a build or installs a browser.
  const wasmPath = join(artifactDir, 'wasm_bindings.wasm')
  const [wasmBytes, wasmStat] = await Promise.all([readFile(wasmPath), stat(wasmPath)])
  const { parsed_activity: activity } = await json(join(root, 'debug/activities/afternoon-ride-parse-debug.json'))
  if (!activity || !Array.isArray(activity.sample_elapsed_seconds) || activity.sample_elapsed_seconds.length < 2)
    throw new Error('The ride fixture is missing its parsed activity timeline')
  const template = await json(join(root, 'templates/futuristic-hud.json'))
  const directory = join(root, 'debug/benchmarks/widget-interactions', `${Date.now()}-${values.label}-${process.pid}`)
  await mkdir(directory, { recursive: true })
  const files = new Map([
    ['/wasm-bindings/wasm_bindings.js', join(artifactDir, 'wasm_bindings.js')],
    ['/wasm-bindings/wasm_bindings.wasm', wasmPath],
    ['/data/activity.json', join(directory, 'activity.json')],
    ['/data/fonts.json', join(directory, 'fonts.json')],
  ])
  await save(files.get('/data/activity.json'), activity)
  const { default: react } = await import(pathToFileURL(appRequire.resolve('@vitejs/plugin-react')).href)
  const server = await createServer({
    configFile: false,
    root,
    publicDir: false,
    resolve: {
      alias: {
        '@': join(root, 'app/src'),
        react: join(root, 'app/node_modules/react'),
        'react-dom': join(root, 'app/node_modules/react-dom'),
        zustand: join(root, 'app/node_modules/zustand'),
      },
    },
    optimizeDeps: { entries: ['benchmarks/widget-interactions/browser.jsx'] },
    plugins: [
      react(),
      {
        name: 'interaction-benchmark',
        enforce: 'pre',
        resolveId(source, importer) {
          if (
            source.replaceAll('\\', '/').endsWith('/app/src/api/backend') &&
            importer?.replaceAll('\\', '/').endsWith('/wasm-widgets/utils/previewSession.js')
          )
            return join(root, 'benchmarks/widget-interactions/backend.js')
        },
        configureServer(server) {
          server.middlewares.use(async (req, res, next) => {
            const path = new URL(req.url, 'http://localhost').pathname
            if (path === '/') {
              res.setHeader('Content-Type', 'text/html')
              res.end(`<!doctype html><html><head><style>
                :root { --app-scale: 1; --theme-color-accent: #ed9238; --theme-color-ice: #aacccc; }
                body { margin: 0; background: #101820; color: white; font: 12px sans-serif; }
                .app-shell, .viewport, .scroll, .stage { width: 100vw; height: 100vh; position: relative; }
                .stage { display: grid; place-items: center; }
                .relative { position: relative; } .absolute, .badge { position: absolute; } .inset-0 { inset: 0; }
                .pointer-events-none, .badge { pointer-events: none; } .overflow-visible { overflow: visible; }
                .z-1 { z-index: 1; } .z-2 { z-index: 2; } .z-3 { z-index: 3; } .z-10 { z-index: 10; }
              </style></head><body><div id="root" class="app-shell"></div>
              <script type="module" src="/benchmarks/widget-interactions/browser.jsx"></script></body></html>`)
              return
            }
            if (!files.has(path)) return next()
            try {
              res.setHeader(
                'Content-Type',
                path.endsWith('.wasm') ? 'application/wasm' : path.endsWith('.js') ? 'text/javascript' : 'application/octet-stream',
              )
              res.end(await readFile(files.get(path)))
            } catch (error) {
              res.statusCode = 500
              res.end(error.message)
            }
          })
        },
      },
    ],
    server: {
      host: '127.0.0.1',
      port: 0,
      hmr: false,
      watch: null,
      headers: { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' },
    },
  })
  let browser
  try {
    const { createFixture } = await server.ssrLoadModule('/benchmarks/widget-interactions/fixture.js')
    const fixtures = protocol.workloads.map((workload) => createFixture({ workload, count: protocol.widgets, activity, template }))
    const fonts = [...new Map(fixtures.flatMap((fixture) => fixture.fonts).map((request) => [JSON.stringify(request), request])).values()]
    const planPath = join(directory, 'plan.json')
    await save(planPath, { root, frames: 1, warmup: 0, fps: 30, activities: {}, cases: [], fonts })
    await execute(join(root, 'src-tauri/target/release', `benchmark_widgets${process.platform === 'win32' ? '.exe' : ''}`), ['--fonts', planPath], {
      cwd: root,
      windowsHide: true,
    })
    for (const font of await json(files.get('/data/fonts.json'))) files.set(`/data/${font.path}`, join(directory, font.path))
    await save(join(directory, 'fixtures.json'), fixtures)
    await server.listen()
    browser = await chromium.launch({ channel: 'chromium', headless: true })
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 })
    page.on('pageerror', (error) => console.error(`[browser] ${error.message}`))
    const { stdout: commit } = await execute('git', ['rev-parse', 'HEAD'], { cwd: root, windowsHide: true })
    const { stdout: dirty } = await execute('git', ['status', '--short'], { cwd: root, windowsHide: true })
    const output = {
      schema: 1,
      label: values.label,
      protocol,
      environment: {
        cpu: cpus()[0].model,
        os: platform(),
        browser: browser.version(),
        viewport: [1600, 1000],
        commit: commit.trim(),
        working_tree: dirty.trim(),
        rect_stacks: values['rect-stacks'],
        wasm: { path: wasmPath, sha256: createHash('sha256').update(wasmBytes).digest('hex'), modified_at: wasmStat.mtime.toISOString() },
      },
      results: [],
    }
    for (const fixture of fixtures) {
      for (const operation of protocol.operations) {
        console.info(`[interactions] ${fixture.workload}-${operation}: ${protocol.widgets} widgets`)
        await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/${values['rect-stacks'] ? '?rect-stacks' : ''}`)
        await page.waitForFunction(() => window.runInteractionBenchmark !== undefined, undefined, { timeout: 60000 })
        const row = await page.evaluate((input) => window.runInteractionBenchmark(input), {
          fixture,
          activity,
          operation,
          frames: protocol.frames,
          warmup: protocol.warmup,
          burst: protocol.burst,
          count: protocol.widgets,
        })
        output.results.push(row)
        const report = summary(output.results, baseline)
        await save(join(directory, 'results.json'), output)
        await writeFile(join(directory, 'summary.md'), report)
        const frame = stats(row.frame_intervals_ms)
        console.info(
          `  first response ${row.first_presentation_ms.toFixed(2)} ms; frame p95 ${frame.p95_ms.toFixed(2)} ms; config updates ${row.stages['wasm.config_update']?.length ?? 0}`,
        )
      }
    }
    console.info(`Results: ${directory}`)
  } finally {
    if (browser) await browser.close()
    await server.close()
  }
}

main().catch((error) => {
  console.error(error.stack)
  process.exitCode = 1
})
