import { parseArgs, promisify } from 'node:util'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { cpus, platform } from 'node:os'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const binary = join(root, 'target/release', `benchmark_interactions${process.platform === 'win32' ? '.exe' : ''}`)
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
    `${protocol.widgets} selected widgets; ${protocol.frames} measured input frames; ${protocol.warmup} warm-up frames; ${protocol.burst} pointer moves per frame. Playhead paused at 60 seconds.`,
    '',
    '| Case | First input → group geometry ms | Frame avg ms | p95 ms | Max ms | Frames >16.7 ms | Widget redraws | Redrawn MiB | Release ms |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |',
  ]
  for (const row of results) {
    const frame = stats(row.frame_ms)
    lines.push(
      `| ${row.id} | ${row.first_presentation_ms.toFixed(2)} | ${frame.avg_ms.toFixed(2)} | ${frame.p95_ms.toFixed(2)} | ${frame.max_ms.toFixed(2)} | ${row.frame_ms.filter((ms) => ms > 1000 / 60).length} | ${row.counters.redraws} | ${(row.counters.redrawn_bytes / 1048576).toFixed(2)} | ${row.release_ms.toFixed(2)} |`,
    )
  }
  lines.push(
    '',
    'A frame is the CPU time of its pointer moves and of the preview redraw that follows them. The first-input probe and the release are timed on their own.',
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
  }
  lines.push(
    '',
    '`update` is one pointer move through the update loop (the action, the reconcile steps and the history step). `preview` is one redraw: config sync, layout and Skia raster. The window is not measured: projections, uploads and presentation.',
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
        ['Frame p95', stats(before.frame_ms).p95_ms, stats(row.frame_ms).p95_ms],
        ['Release', before.release_ms, row.release_ms],
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
      baseline.schema !== 2 ||
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
        !Array.isArray(row.frame_ms) ||
        row.frame_ms.length !== protocol.frames ||
        !row.frame_ms.every((ms) => Number.isFinite(ms) && ms >= 0)
      )
        throw new Error(`Invalid baseline frame measurements for ${row.id}`)
      for (const samples of Object.values(row.stages)) {
        if (!Array.isArray(samples) || samples.length === 0 || !samples.every((ms) => Number.isFinite(ms) && ms >= 0))
          throw new Error(`Invalid baseline stage samples for ${row.id}`)
      }
    }
    if (expected.size !== 0) throw new Error(`Baseline is missing cases: ${[...expected].join(', ')}`)
  }
  // The binary must already exist. This runner never invokes a build.
  const [binaryBytes, binaryStat] = await Promise.all([readFile(binary), stat(binary)])
  const directory = join(root, 'debug/benchmarks/widget-interactions', `${Date.now()}-${values.label}-${process.pid}`)
  await mkdir(directory, { recursive: true })
  const { stdout: commit } = await execute('git', ['rev-parse', 'HEAD'], { cwd: root, windowsHide: true })
  const { stdout: dirty } = await execute('git', ['status', '--short'], { cwd: root, windowsHide: true })
  const output = {
    schema: 2,
    label: values.label,
    protocol,
    environment: {
      cpu: cpus()[0].model,
      os: platform(),
      commit: commit.trim(),
      working_tree: dirty.trim(),
      binary: { path: binary, sha256: createHash('sha256').update(binaryBytes).digest('hex'), modified_at: binaryStat.mtime.toISOString() },
    },
    results: [],
  }
  for (const workload of protocol.workloads) {
    for (const operation of protocol.operations) {
      console.info(`[interactions] ${workload}-${operation}: ${protocol.widgets} widgets`)
      const path = join(directory, `${workload}-${operation}.json`)
      const counts = [protocol.widgets, protocol.frames, protocol.warmup, protocol.burst].map(String)
      await execute(binary, [root, path, ...counts, workload, operation], { cwd: root, windowsHide: true, maxBuffer: 1 << 26 })
      const row = await json(path)
      output.results.push(row)
      await save(join(directory, 'results.json'), output)
      await writeFile(join(directory, 'summary.md'), summary(output.results, baseline))
      console.info(`  first response ${row.first_presentation_ms.toFixed(2)} ms; frame p95 ${stats(row.frame_ms).p95_ms.toFixed(2)} ms`)
    }
  }
  console.info(`Results: ${directory}`)
}

main().catch((error) => {
  console.error(error.stack)
  process.exitCode = 1
})
