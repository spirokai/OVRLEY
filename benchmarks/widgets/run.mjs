import { parseArgs } from 'node:util'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { cpus, platform } from 'node:os'
import { formatSummary } from './summarize.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const { values } = parseArgs({
  options: {
    build: { type: 'boolean' },
    run: { type: 'boolean' },
    plan: { type: 'boolean' },
    list: { type: 'boolean' },
    frames: { type: 'string', default: '1000' },
    warmup: { type: 'string', default: '200' },
    backend: { type: 'string', default: 'both' },
    case: { type: 'string' },
  },
})
const frames = Number(values.frames),
  warmup = Number(values.warmup),
  fps = 30
if (
  !/^\d+$/.test(values.frames) ||
  !Number.isSafeInteger(frames) ||
  frames < 1 ||
  !/^\d+$/.test(values.warmup) ||
  !Number.isSafeInteger(warmup) ||
  !Number.isSafeInteger(frames + warmup)
)
  throw new Error('frames must be positive and warmup non-negative integers')
if (
  !['both', 'native', 'native-preview', 'wasm'].includes(values.backend) ||
  ['build', 'run', 'plan', 'list'].filter((key) => values[key]).length > 1
)
  throw new Error('Choose one mode and backend: both, native, native-preview, or wasm')
const json = async (path) => JSON.parse(await readFile(path, 'utf8'))
const save = (path, value) => writeFile(path, JSON.stringify(value, null, 2) + '\n')
const fingerprint = async (path) => {
  const [bytes, info] = await Promise.all([readFile(path), stat(path)])
  return { path, sha256: createHash('sha256').update(bytes).digest('hex'), modified_at: info.mtime.toISOString() }
}
const child = (binary, args, cwd = root) =>
  new Promise((done, fail) => {
    const process = spawn(binary, args, { cwd, stdio: 'inherit', windowsHide: true })
    process.once('error', fail)
    process.once('close', (code) => (code === 0 ? done() : fail(new Error(`${binary} exited with ${code}`))))
  })

async function main() {
  if (values.build) {
    await child('cargo', ['build', '-p', 'ovrley_core', '--release', '--bin', 'benchmark_widgets', '--locked'], join(root, 'src-tauri'))
    await child(process.execPath, [join(root, 'scripts/build-wasm.mjs')])
    return
  }
  const { createServer } = await import('vite-plus')
  const { artifactDir } = await import('../../scripts/build-wasm.mjs')
  const files = new Map(['js', 'wasm'].map((ext) => [`/wasm-bindings/wasm_bindings.${ext}`, join(artifactDir, `wasm_bindings.${ext}`)]))
  const serve = async (req, res, next) => {
    const path = new URL(req.url, 'http://localhost').pathname
    if (path === '/') {
      res.setHeader('Content-Type', 'text/html')
      res.end('<canvas></canvas><script type="module">import {play} from "/benchmarks/widgets/browser.js"; window.play = play</script>')
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
  }
  const server = await createServer({
    configFile: false,
    root,
    publicDir: false,
    optimizeDeps: { entries: ['benchmarks/widgets/browser.js'] },
    resolve: { alias: { '@': join(root, 'app/src') } },
    plugins: [
      {
        name: 'widget-benchmark',
        configureServer: (server) => {
          server.middlewares.use(serve)
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
  try {
    const { createCases, fixtures } = await server.ssrLoadModule('/benchmarks/widgets/browser.js')
    const activities = Object.fromEntries(
      await Promise.all(
        Object.entries(fixtures).map(async ([id, [name]]) => {
          const { parsed_activity: activity } = await json(join(root, 'debug/activities', name))
          if (!activity || !Array.isArray(activity.sample_elapsed_seconds) || activity.sample_elapsed_seconds.length < 2)
            throw new Error(`${name}: missing parsed activity timeline`)
          return [id, activity]
        }),
      ),
    )
    let cases = createCases({
      activities,
      template: await json(join(root, 'templates/futuristic-hud.json')),
      image: join(root, 'src-tauri/icons/128x128@2x.png'),
      frames,
      warmup,
      fps,
    })
    if (values.case) {
      if (!cases.some((entry) => entry.id === values.case)) throw new Error(`Unknown case: ${values.case}`)
      cases = cases.filter((entry) => entry.id === 'empty' || entry.id === values.case)
    }
    if (!values.run && !values.plan) {
      console.info(cases.map((entry) => entry.id).join('\n'))
      return
    }
    const directory = join(root, 'debug/benchmarks/widgets', `${Date.now()}-${process.pid}`)
    await mkdir(directory, { recursive: true })
    const paths = {}
    for (const [id, activity] of Object.entries(activities)) {
      paths[id] = join(directory, `${id}.json`)
      await save(paths[id], activity)
      files.set(`/data/${id}.json`, paths[id])
    }
    const fonts = [...new Map(cases.flatMap((entry) => entry.fonts).map((request) => [JSON.stringify(request), request])).values()]
    const plan = { root, frames, warmup, fps, activities: paths, cases, fonts }
    const planPath = join(directory, 'plan.json')
    await save(planPath, plan)
    console.info(`Plan: ${planPath}`)
    if (!values.run) return
    const binary = join(root, 'src-tauri/target/release', `benchmark_widgets${process.platform === 'win32' ? '.exe' : ''}`)
    await child(binary, ['--fonts', planPath])
    const resources = await json(join(directory, 'fonts.json'))
    for (const font of resources) files.set(`/data/${font.path}`, join(directory, font.path))
    for (const entry of cases) if (entry.split_table !== null) files.set(`/data/${entry.split_table}`, join(directory, entry.split_table))
    files.set('/data/image.png', join(root, 'src-tauri/icons/128x128@2x.png'))
    const results = []
    const environment = { cpu: cpus()[0].model, os: platform(), artifacts: { native: await fingerprint(binary) } }
    if (['both', 'wasm'].includes(values.backend)) environment.artifacts.wasm = await fingerprint(join(artifactDir, 'wasm_bindings.wasm'))
    if (['both', 'native'].includes(values.backend)) {
      await child(binary, ['--run', planPath])
      results.push(...(await json(join(directory, 'native.json'))))
    }
    if (['both', 'native-preview'].includes(values.backend)) {
      await child(binary, ['--run-preview', planPath])
      results.push(...(await json(join(directory, 'native-preview.json'))))
    }
    if (['both', 'wasm'].includes(values.backend)) {
      const { chromium } = await import('@playwright/test')
      await server.listen()
      const browser = await chromium.launch({ channel: 'chromium', headless: true })
      environment.browser = browser.version()
      try {
        const page = await browser.newPage({ viewport: { width: 3840, height: 2160 }, deviceScaleFactor: 1 })
        await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/`)
        await page.waitForFunction(() => window.play !== undefined)
        for (const entry of cases) {
          console.info(`[wasm] ${entry.id}`)
          results.push(await page.evaluate((input) => window.play(input), { entry, frames, warmup, fps, fonts: resources }))
          await save(
            join(directory, 'wasm.json'),
            results.filter((result) => result.backend === 'wasm'),
          )
          await save(join(directory, 'results.json'), { plan, environment, results })
        }
      } finally {
        await browser.close()
      }
    }
    await save(join(directory, 'results.json'), { plan, environment, results })
    await writeFile(join(directory, 'summary.md'), formatSummary(results))
    console.info(`Results: ${directory}`)
  } finally {
    await server.close()
  }
}
main().catch((error) => {
  console.error(error.message)
  process.exitCode = 1
})
