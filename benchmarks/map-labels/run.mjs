// Map label benchmark coordinator. See README.md.
import { parseArgs } from 'node:util'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'
import { existsSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { cpus, platform } from 'node:os'
import { formatSummary } from './summarize.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const binary = join(root, 'src-tauri/target/release/benchmark_map_labels')
const tilesDir = join(root, 'debug/benchmarks/map-labels/tiles')
const tileUrl = 'https://tiles.openfreemap.org/planet/latest/{z}/{x}/{y}.pbf'
const { values } = parseArgs({
  options: {
    build: { type: 'boolean' },
    fetch: { type: 'boolean' },
    run: { type: 'boolean' },
    diagnose: { type: 'boolean' },
    verify: { type: 'boolean' },
    frames: { type: 'string', default: '600' },
    warmup: { type: 'string', default: '120' },
    backend: { type: 'string', default: 'both' },
  },
})
if (!['both', 'native', 'wasm'].includes(values.backend)) throw new Error('backend: both, native or wasm')
if (!/^\d+$/.test(values.frames) || !/^\d+$/.test(values.warmup) || Number(values.frames) < 1) throw new Error('frames and warmup are integers')

const child = (command, args, cwd = root) =>
  new Promise((done, fail) => {
    const process = spawn(command, args, { cwd, stdio: 'inherit' })
    process.once('error', fail)
    process.once('close', (code) => (code === 0 ? done() : fail(new Error(`${command} exited with ${code}`))))
  })
const plan = (path) => child(binary, ['--plan', path, values.frames, values.warmup])

async function build() {
  await child(
    'cargo',
    ['build', '-p', 'ovrley_map', '--release', '--features', 'gpu,validate', '--bin', 'benchmark_map_labels', '--locked'],
    join(root, 'src-tauri'),
  )
  const { buildMapBindings } = await import('../../scripts/build-wasm.mjs')
  buildMapBindings()
}

/** Downloads the plan's tiles that aren't on disk yet. */
async function fetchTiles() {
  const dir = join(root, 'debug/benchmarks/map-labels')
  await mkdir(dir, { recursive: true })
  await plan(join(dir, 'plan-fetch.json'))
  const { tiles } = JSON.parse(await readFile(join(dir, 'plan-fetch.json'), 'utf8'))
  const missing = tiles.filter(([z, x, y]) => !existsSync(join(tilesDir, `${z}/${x}/${y}.pbf`)))
  console.log(`${tiles.length} tiles, ${missing.length} to download`)
  const queue = [...missing]
  await Promise.all(
    Array.from({ length: 4 }, async () => {
      for (let tile = queue.shift(); tile; tile = queue.shift()) {
        const [z, x, y] = tile
        const response = await fetch(tileUrl.replace('{z}', z).replace('{x}', x).replace('{y}', y), {
          headers: { 'User-Agent': 'OVRLEY-benchmark (+https://www.ovrley.cc)' },
        })
        if (!response.ok) throw new Error(`${z}/${x}/${y}: HTTP ${response.status}`)
        const path = join(tilesDir, `${z}/${x}/${y}.pbf`)
        await mkdir(dirname(path), { recursive: true })
        await writeFile(path, new Uint8Array(await response.arrayBuffer()))
      }
    }),
  )
}

/** Serves the page, the production map module, fonts, style and tiles, cross-origin isolated for fine timers. */
async function serve(planPath) {
  const { artifactDir } = await import('../../scripts/build-wasm.mjs')
  const routes = [
    ['/plan.json', () => planPath],
    ['/style.json', () => join(root, 'assets/map-styles/liberty.json')],
    ['/browser.js', () => join(root, 'benchmarks/map-labels/browser.js')],
    ['/diagnose-browser.js', () => join(root, 'benchmarks/map-labels/diagnose-browser.js')],
    // BENCH_WASM_DIR serves another build of the map module, to compare it with production.
    ['/wasm/', (rest) => join(process.env.BENCH_WASM_DIR ?? artifactDir, rest)],
    ['/fonts/', (rest) => join(root, 'fonts/map', rest)],
    ['/tiles/', (rest) => join(tilesDir, rest)],
  ]
  const types = { js: 'text/javascript', wasm: 'application/wasm', json: 'application/json', html: 'text/html' }
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname)
    res.setHeader('Cross-Origin-Opener-Policy', 'same-origin')
    res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp')
    if (path === '/') {
      res.setHeader('Content-Type', types.html)
      return res.end(
        '<!doctype html><body><script type="module">const page = new URL(location.href).searchParams.get("diagnose") ? await import("/diagnose-browser.js") : await import("/browser.js"); window.play = page.play; window.diagnose = page.diagnose; window.raw = page.raw</script>',
      )
    }
    const route = routes.find(([prefix]) => (prefix.endsWith('/') ? path.startsWith(prefix) : path === prefix))
    if (!route || path.includes('..')) return void ((res.statusCode = 404), res.end())
    try {
      const file = route[1](path.slice(route[0].length))
      res.setHeader('Content-Type', types[file.split('.').pop()] ?? 'application/octet-stream')
      res.end(await readFile(file))
    } catch (error) {
      res.statusCode = 404
      res.end(error.message)
    }
  })
  await new Promise((done) => server.listen(0, '127.0.0.1', done))
  return server
}

// Headless Chromium on the hardware GPU through ANGLE's Vulkan backend, without vsync or frame limits.
const launch = async () => {
  const { chromium } = await import('@playwright/test')
  return chromium.launch({
    headless: !process.env.BENCH_HEADED,
    args: [
      ...(process.env.BENCH_ANGLE === 'gl' ? ['--use-angle=gl'] : ['--use-angle=vulkan', '--enable-features=Vulkan']),
      '--ignore-gpu-blocklist',
      '--enable-gpu',
      '--disable-gpu-vsync',
      '--disable-frame-rate-limit',
    ],
  })
}

async function runWasm(planPath) {
  const server = await serve(planPath)
  const browser = await launch()
  try {
    const page = await browser.newPage()
    page.on('console', (message) => console.log(message.text()))
    page.on('pageerror', (error) => console.error(error))
    await page.goto(`http://127.0.0.1:${server.address().port}/`)
    await page.waitForFunction(() => window.play)
    const result = await page.evaluate(() => window.play())
    result.browser = browser.version()
    return result
  } finally {
    await browser.close()
    server.close()
  }
}

/** Where WebGL frames spend their time: variants, a census of WebGL calls, and a Chrome trace. */
async function diagnose() {
  const dir = join(root, 'debug/benchmarks/map-labels', `diagnose-${Date.now()}`)
  await mkdir(dir, { recursive: true })
  console.log(`Results: ${dir}`)
  const planPath = join(dir, 'plan.json')
  await plan(planPath)
  const server = await serve(planPath)
  const browser = await launch()
  const results = []
  try {
    const page = await browser.newPage()
    page.on('pageerror', (error) => console.error(error))
    await page.goto(`http://127.0.0.1:${server.address().port}/?diagnose=1`)
    await page.waitForFunction(() => window.diagnose)
    const frames = Number(values.frames),
      warmup = Number(values.warmup)
    const renderer = await page.evaluate(() => {
      const gl = document.createElement('canvas').getContext('webgl2')
      return gl.getParameter(gl.getExtension('WEBGL_debug_renderer_info').UNMASKED_RENDERER_WEBGL)
    })
    console.log(`WebGL renderer: ${renderer}`)
    if (values.verify) {
      // The quick check: frame time with and without buildings, and the last frame for pixel comparison.
      console.log('| Case | Buildings | Draw (CPU) | GPU wait | Frame |')
      console.log('| --- | --- | --- | --- | --- |')
      for (const caseId of ['urban-1000-chase', 'urban-cjk-1000-chase', 'urban-cjk-1000-top-down', 'rural-400-top-down'])
        for (const buildings of [true, false]) {
          const result = await page.evaluate((run) => window.diagnose(run), {
            caseId,
            labels: true,
            msaa: 4,
            buildings,
            warmup,
            frames,
            snapshot: buildings,
          })
          if (result.snapshot) await writeFile(join(dir, `${caseId}.rgba`), Buffer.from(result.snapshot, 'base64'))
          delete result.snapshot
          results.push({ caseId, buildings, ...result })
          const ms = (value) => value.toFixed(2)
          console.log(
            `| ${caseId} | ${buildings ? 'on' : 'off'} | ${ms(result.draw_ms)} | ${ms(result.sync_ms)} | ${ms(result.draw_ms + result.sync_ms)} |`,
          )
        }
      return
    }
    for (const [indices, draws] of [
      [2.27e6, 13],
      [5.04e6, 11],
      [2.27e6, 547],
    ])
      for (const usage of ['STATIC_DRAW', 'DYNAMIC_DRAW', 'STREAM_DRAW'])
        for (const layout of ['plain', 'map']) {
          const ms = await page.evaluate((run) => window.raw(run), { indices, draws, layout, usage })
          console.log(`raw WebGL ${usage} ${layout}: ${(indices / 1e6).toFixed(2)} M indices in ${draws} draws: ${ms.toFixed(2)} ms`)
          results.push({ raw: { indices, draws, layout, usage, ms } })
        }
    if (process.env.BENCH_RAW_ONLY) return
    if (process.env.BENCH_SNAPSHOT) {
      for (const caseId of ['urban-1000-chase', 'urban-cjk-1000-chase']) {
        const result = await page.evaluate((run) => window.diagnose(run), {
          caseId,
          labels: true,
          msaa: 4,
          buildings: true,
          warmup: 0,
          frames: 60,
          snapshot: true,
        })
        await writeFile(join(dir, `${caseId}.rgba`), Buffer.from(result.snapshot, 'base64'))
      }
      return
    }
    for (const caseId of ['urban-1000-chase', 'urban-cjk-1000-chase', 'urban-cjk-1000-top-down', 'rural-400-top-down']) {
      for (const [variant, msaa, buildings] of [
        ['base', 4, true],
        ['msaa 1', 1, true],
        ['no buildings', 4, false],
        ['msaa 1, no buildings', 1, false],
      ]) {
        const run = { caseId, labels: false, msaa, buildings, warmup, frames, census: false }
        const result = await page.evaluate((run) => window.diagnose(run), run)
        console.log(
          `${caseId} ${variant}: draw ${result.draw_ms.toFixed(2)} sync ${result.sync_ms.toFixed(2)} idle sync ${result.idle_sync_ms.toFixed(3)}`,
        )
        results.push({ caseId, variant, ...result })
      }
      for (const buildings of [true, false]) {
        const census = await page.evaluate((run) => window.diagnose(run), { caseId, labels: false, msaa: 4, buildings, warmup, frames, census: true })
        const draws = census.census.drawElementsInstanced
        console.log(
          `${caseId} census buildings=${buildings}: ${draws.calls} indexed draws, ${(draws.bytes / 1e6).toFixed(2)} M indices, largest ${draws.largest}`,
        )
        results.push({ caseId, variant: `census${buildings ? '' : ', no buildings'}`, ...census })
      }
    }
    await browser.startTracing(page, {
      path: join(dir, 'trace.json'),
      categories: [
        'gpu',
        'gpu.service',
        'disabled-by-default-gpu.service',
        'disabled-by-default-gpu.device',
        'gpu.angle',
        'toplevel',
        'viz',
        'blink',
        'v8',
      ],
    })
    await page.evaluate((run) => window.diagnose(run), {
      caseId: 'urban-1000-chase',
      labels: false,
      msaa: 4,
      buildings: true,
      warmup: 30,
      frames: 120,
      census: false,
    })
    await browser.stopTracing()
  } finally {
    await browser.close()
    server.close()
    await writeFile(join(dir, 'diagnose.json'), JSON.stringify(results, null, 2) + '\n')
  }
}

async function main() {
  if (values.build) return build()
  if (values.diagnose || values.verify) return diagnose()
  if (values.fetch) return fetchTiles()
  if (!values.run) {
    console.log('Use --build, --fetch, --run [--backend both|native|wasm] [--frames N] [--warmup N] or --diagnose')
    return
  }
  const dir = join(root, 'debug/benchmarks/map-labels', `${Date.now()}-${process.pid}`)
  await mkdir(dir, { recursive: true })
  const planPath = join(dir, 'plan.json')
  await plan(planPath)
  if (values.backend !== 'wasm') await child(binary, ['--run', planPath, tilesDir, join(dir, 'native.json')])
  if (values.backend !== 'native') await writeFile(join(dir, 'wasm.json'), JSON.stringify(await runWasm(planPath), null, 2) + '\n')
  const environment = {
    platform: platform(),
    cpu: cpus()[0]?.model,
    cores: cpus().length,
    frames: Number(values.frames),
    warmup: Number(values.warmup),
  }
  await writeFile(join(dir, 'environment.json'), JSON.stringify(environment, null, 2) + '\n')
  const summary = await formatSummary(dir)
  await writeFile(join(dir, 'summary.md'), summary)
  console.log(summary)
  console.log(`Results: ${dir}`)
}

await main()
