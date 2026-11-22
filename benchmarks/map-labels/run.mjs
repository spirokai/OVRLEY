// Map label benchmark coordinator. See README.md.
import { parseArgs } from 'node:util'
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { cpus, platform } from 'node:os'
import { formatSummary } from './summarize.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const binary = join(root, 'target/release', `benchmark_map_labels${process.platform === 'win32' ? '.exe' : ''}`)
const tilesDir = join(root, 'debug/benchmarks/map-labels/tiles')
const tileUrl = 'https://tiles.openfreemap.org/planet/latest/{z}/{x}/{y}.pbf'
const { values } = parseArgs({
  options: {
    build: { type: 'boolean' },
    fetch: { type: 'boolean' },
    run: { type: 'boolean' },
    frames: { type: 'string', default: '600' },
    warmup: { type: 'string', default: '120' },
  },
})
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
    root,
  )
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

async function main() {
  if (values.build) return build()
  if (values.fetch) return fetchTiles()
  if (!values.run) {
    console.log('Use --build, --fetch or --run [--frames N] [--warmup N]')
    return
  }
  const dir = join(root, 'debug/benchmarks/map-labels', `${Date.now()}-${process.pid}`)
  await mkdir(dir, { recursive: true })
  const planPath = join(dir, 'plan.json')
  await plan(planPath)
  await child(binary, ['--run', planPath, tilesDir, join(dir, 'native.json')])
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
