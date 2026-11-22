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
  warmup = Number(values.warmup)
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
  !['both', 'native', 'native-preview'].includes(values.backend) ||
  ['build', 'run', 'plan', 'list'].filter((key) => values[key]).length > 1
)
  throw new Error('Choose one mode and backend: both, native, or native-preview')
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
    await child('cargo', ['build', '-p', 'ovrley_app', '--release', '--bin', 'benchmark_widgets', '--locked'])
    return
  }
  const binary = join(root, 'target/release', `benchmark_widgets${process.platform === 'win32' ? '.exe' : ''}`)
  if (!values.run && !values.plan) {
    await child(binary, ['--list'])
    return
  }
  const directory = join(root, 'debug/benchmarks/widgets', `${Date.now()}-${process.pid}`)
  await mkdir(directory, { recursive: true })
  const planPath = join(directory, 'plan.json')
  await child(binary, ['--plan', directory, root, String(frames), String(warmup), ...(values.case === undefined ? [] : [values.case])])
  console.info(`Plan: ${planPath}`)
  if (!values.run) return
  await child(binary, ['--fonts', planPath])
  const results = []
  for (const [backend, mode] of [
    ['native', '--run'],
    ['native-preview', '--run-preview'],
  ]) {
    if (!['both', backend].includes(values.backend)) continue
    await child(binary, [mode, planPath])
    results.push(...(await json(join(directory, `${backend}.json`))))
  }
  const environment = { cpu: cpus()[0].model, os: platform(), artifacts: { native: await fingerprint(binary) } }
  await save(join(directory, 'results.json'), { plan: await json(planPath), environment, results })
  await writeFile(join(directory, 'summary.md'), formatSummary(results))
  console.info(`Results: ${directory}`)
}
main().catch((error) => {
  console.error(error.message)
  process.exitCode = 1
})
