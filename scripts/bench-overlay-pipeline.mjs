#!/usr/bin/env node
/**
 * Measures where composite export time goes, per hardware backend, using the
 * same FFmpeg filter graphs as `ovrley_core/src/encode/ffmpeg/composite_profiles.rs`.
 * A separate FFmpeg process stands in for the Skia renderer and writes raw
 * overlay frames to the encoder's stdin, like the app does.
 *
 * Usage:
 *   node scripts/bench-overlay-pipeline.mjs [--source debug/benchmarks/test-4k.mp4]
 *     [--seconds 20] [--codec hevc|h264] [--backends amf,nvenc,cuda,qsv,qsvfull,software]
 *     [--quality 23] [--ffmpeg path/to/ffmpeg]
 *
 * Backends (unavailable ones are skipped):
 *   software  CPU overlay, libx264/libx265
 *   amf       CPU overlay, nv12 + hwupload to D3D11, AMF encode       (AMF_D3D11_FILTER)
 *   nvenc     CPU overlay, NVENC encode                               (NvgpuH264/Hevc)
 *   cuda      CUDA decode, yuva420p + hwupload, overlay_cuda, NVENC   (CUDA_*_FILTER)
 *   qsv       CPU overlay, QSV encode                                 (QsvH264/Hevc)
 *   qsvfull   QSV decode, RGBA hwupload, overlay_qsv, QSV encode      (QSV_FULL_FILTER)
 *
 * Variants per backend:
 *   no-overlay      decode + encode only (the floor)
 *   app-today       full-frame RGBA pipe at the source frame rate (update rate 1)
 *   half-rate       full-frame RGBA pipe at half the frame rate (update rate 2)
 *   yuva420p        full-frame yuva420p pipe (no RGBA -> YUVA conversion in FFmpeg)
 *   cropped         RGBA pipe of only the widget box, placed with x/y
 */

import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const exe = process.platform === 'win32' ? '.exe' : ''

function parseArgs(argv) {
  const options = {
    source: path.join(repoRoot, 'debug/benchmarks/test-4k.mp4'),
    seconds: 20,
    codec: 'hevc',
    backends: ['software', 'amf', 'nvenc', 'cuda', 'qsv', 'qsvfull'],
    quality: 23,
    ffmpeg: path.join(repoRoot, `vendor/ffmpeg/bin/ffmpeg${exe}`),
  }
  for (let index = 0; index < argv.length; index += 2) {
    const [flag, value] = [argv[index], argv[index + 1]]
    if (value === undefined) throw new Error(`Missing value for ${flag}`)
    if (flag === '--source') options.source = path.resolve(value)
    else if (flag === '--seconds') options.seconds = Number(value)
    else if (flag === '--codec') options.codec = value
    else if (flag === '--backends') options.backends = value.split(',')
    else if (flag === '--quality') options.quality = Number(value)
    else if (flag === '--ffmpeg') options.ffmpeg = path.resolve(value)
    else throw new Error(`Unknown option ${flag}`)
  }
  if (!['hevc', 'h264'].includes(options.codec)) throw new Error('--codec must be hevc or h264')
  if (!Number.isFinite(options.seconds) || options.seconds <= 0) throw new Error('--seconds must be positive')
  if (!existsSync(options.ffmpeg)) throw new Error(`FFmpeg not found: ${options.ffmpeg}`)
  if (!existsSync(options.source)) throw new Error(`Source video not found: ${options.source} (pass a 4K clip with --source)`)
  return options
}

function probeSource(ffmpeg, source) {
  const ffprobe = path.join(path.dirname(ffmpeg), `ffprobe${exe}`)
  const result = spawnSync(
    ffprobe,
    ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height,r_frame_rate,codec_name', '-of', 'json', source],
    { encoding: 'utf8' },
  )
  if (result.status !== 0) throw new Error(`ffprobe failed: ${result.stderr}`)
  const [stream] = JSON.parse(result.stdout).streams
  const [num, den] = stream.r_frame_rate.split('/').map(Number)
  return { width: stream.width, height: stream.height, fps: { num, den }, codec: stream.codec_name }
}

/** Encoder name and arguments per backend, mirroring the profile output and quality args. */
function encoderArgs(backend, codec, quality) {
  const q = String(quality)
  const nvencAq = ['-bf:v', '3', '-spatial-aq', 'true', '-temporal-aq', 'true', '-rc:v', 'vbr', '-b:v', '0', '-cq:v', q]
  switch (backend) {
    case 'software':
      return codec === 'hevc'
        ? ['-c:v', 'libx265', '-pix_fmt', 'yuv420p10le', '-profile:v', 'main10', '-preset', 'veryfast', '-crf', q]
        : ['-c:v', 'libx264', '-preset', 'veryfast', '-crf', q]
    case 'amf':
      return codec === 'hevc'
        ? ['-c:v', 'hevc_amf', '-rc', 'cqp', '-qp_i', q, '-qp_p', q]
        : ['-c:v', 'h264_amf', '-rc', 'cqp', '-qp_i', q, '-qp_p', q, '-qp_b', q]
    case 'nvenc':
      return [`-c:v`, `${codec}_nvenc`, ...(codec === 'h264' ? ['-profile:v', 'high'] : []), ...nvencAq]
    case 'cuda':
      return [`-c:v`, `${codec}_nvenc`, ...(codec === 'h264' ? ['-profile:v', 'main'] : []), ...nvencAq]
    case 'qsv':
    case 'qsvfull':
      return ['-c:v', `${codec}_qsv`, '-b:v', '0', '-global_quality', q]
    default:
      throw new Error(`Unknown backend ${backend}`)
  }
}

/** Input arguments and filter graph per backend, mirroring composite_profiles.rs. */
function pipeline(backend, codec, { width, height, seconds, overlay }) {
  const trim = `trim=start=0:end=${seconds},setpts=PTS-STARTPTS,`
  const place = `${overlay.x}:${overlay.y}`
  const cpuOverlay = (tail) =>
    `[0:v]${trim}null[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=${place}:eof_action=repeat:shortest=1${tail}[out]`
  const cpuFilter = codec === 'h264' ? cpuOverlay(',format=yuv420p') : cpuOverlay('')
  switch (backend) {
    case 'software':
    case 'nvenc':
    case 'qsv':
      return { inputArgs: [], filter: cpuFilter, noOverlay: `[0:v]${trim}null[out]` }
    case 'amf':
      return {
        inputArgs: ['-init_hw_device', 'd3d11va=dx', '-filter_hw_device', 'dx'],
        filter: cpuOverlay(',format=nv12,hwupload'),
        noOverlay: `[0:v]${trim}format=nv12,hwupload[out]`,
      }
    case 'cuda':
      return {
        inputArgs: ['-init_hw_device', 'cuda=cuda', '-filter_hw_device', 'cuda', '-hwaccel', 'cuda', '-hwaccel_output_format', 'cuda'],
        filter:
          `[0:v]${trim}scale_cuda=w=${width}:h=${height}:format=yuv420p[base];` +
          `[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];[base][ovr]overlay_cuda=${place}:eof_action=repeat:shortest=1[out]`,
        noOverlay: `[0:v]${trim}scale_cuda=w=${width}:h=${height}:format=yuv420p[out]`,
      }
    case 'qsvfull':
      return {
        inputArgs: ['-init_hw_device', 'qsv=qs', '-filter_hw_device', 'qs', '-hwaccel', 'qsv', '-hwaccel_output_format', 'qsv'],
        filter:
          `[0:v]${trim}scale_qsv=w=${width}:h=${height}:format=nv12[main_hw];` +
          `[1:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=16[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=${overlay.x}:y=${overlay.y}[out]`,
        noOverlay: `[0:v]${trim}scale_qsv=w=${width}:h=${height}:format=nv12[out]`,
      }
    default:
      throw new Error(`Unknown backend ${backend}`)
  }
}

function fpsArg({ num, den }) {
  return `${num}/${den}`
}

/**
 * Runs the overlay producer piped into the encoder and returns wall time and encoder CPU time.
 * The producer's stdout handle is passed to the encoder as its stdin, so frames travel through
 * one OS pipe between the two processes (as from the app's renderer), never through Node.
 */
function runPipeline({ ffmpeg, producerArgs, encoderArgs: args }) {
  return new Promise((resolve) => {
    const started = performance.now()
    let producer = null
    let producerError = ''
    if (producerArgs) {
      producer = spawn(ffmpeg, producerArgs, { stdio: ['ignore', 'pipe', 'pipe'] })
      producer.stderr.on('data', (chunk) => (producerError += chunk))
    }
    const encoder = spawn(ffmpeg, args, { stdio: [producer ? producer.stdout : 'ignore', 'ignore', 'pipe'] })
    // The encoder owns the read end now; Node must not consume it.
    producer?.stdout.pause()
    let stderr = ''
    encoder.stderr.on('data', (chunk) => (stderr += chunk))
    encoder.on('close', (code) => {
      producer?.kill()
      const seconds = (performance.now() - started) / 1000
      const bench = /utime=([\d.]+)s stime=([\d.]+)s rtime=([\d.]+)s/.exec(stderr)
      const cpuSeconds = bench ? Number(bench[1]) + Number(bench[2]) : null
      const errors = stderr
        .split('\n')
        .filter((line) => /error|failed|invalid|cannot|not supported/i.test(line))
        .concat(producerError ? [producerError.trim()] : [])
      resolve({ ok: code === 0, seconds, cpuSeconds, errors: errors.slice(0, 3).join(' | ') })
    })
  })
}

function producerArgsFor({ width, height, fps, frames, pixFmt, box }) {
  const source = `color=c=black@0:s=${width}x${height}:r=${fpsArg(fps)},format=rgba,drawbox=x=${box.x}:y=${box.y}:w=${box.w}:h=${box.h}:color=white@0.6:t=fill`
  return [
    '-hide_banner',
    '-v',
    'error',
    '-nostdin',
    '-f',
    'lavfi',
    '-i',
    source,
    '-frames:v',
    String(frames),
    '-f',
    'rawvideo',
    '-pix_fmt',
    pixFmt,
    '-',
  ]
}

function encodeArgsFor({ options, backend, source, filter, overlayInput, output }) {
  const { inputArgs } = pipeline(backend, options.codec, {
    width: source.width,
    height: source.height,
    seconds: options.seconds,
    overlay: { x: 0, y: 0 },
  })
  return [
    '-hide_banner',
    '-nostats',
    '-loglevel',
    'info',
    '-benchmark',
    ...inputArgs,
    '-t',
    String(options.seconds + 1),
    '-i',
    options.source,
    ...(overlayInput
      ? [
          '-f',
          'rawvideo',
          '-pix_fmt',
          overlayInput.pixFmt,
          '-s',
          `${overlayInput.width}x${overlayInput.height}`,
          '-r',
          fpsArg(overlayInput.fps),
          '-i',
          'pipe:0',
        ]
      : []),
    '-filter_complex',
    filter,
    '-map',
    '[out]',
    '-an',
    '-r',
    fpsArg(source.fps),
    ...encoderArgs(backend, options.codec, options.quality),
    '-y',
    output,
  ]
}

function encoderAvailable(ffmpeg, backend, codec) {
  const names = {
    software: codec === 'hevc' ? 'libx265' : 'libx264',
    amf: `${codec}_amf`,
    nvenc: `${codec}_nvenc`,
    cuda: `${codec}_nvenc`,
    qsv: `${codec}_qsv`,
    qsvfull: `${codec}_qsv`,
  }
  const listed = spawnSync(ffmpeg, ['-hide_banner', '-encoders'], { encoding: 'utf8' }).stdout.includes(` ${names[backend]} `)
  if (!listed) return `encoder ${names[backend]} not in this FFmpeg build`
  return null
}

function formatRow(cells, widths) {
  return cells.map((cell, index) => String(cell).padEnd(widths[index])).join('  ')
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  const source = probeSource(options.ffmpeg, options.source)
  const frameRate = source.fps.num / source.fps.den
  const sourceFrames = Math.round(options.seconds * frameRate)
  const box = { x: 40, y: 40, w: Math.min(900, source.width - 80), h: Math.min(260, source.height - 80) }
  const outDir = path.join(os.tmpdir(), 'ovrley-overlay-bench')
  mkdirSync(outDir, { recursive: true })

  const lines = []
  const log = (line = '') => {
    console.log(line)
    lines.push(line)
  }
  log(`# Overlay pipeline benchmark`)
  log()
  log(`- Host: ${os.hostname()}, ${os.cpus()[0].model.trim()} (${os.cpus().length} threads), ${process.platform}`)
  log(
    `- Source: ${path.basename(options.source)} (${source.codec} ${source.width}x${source.height} @ ${frameRate.toFixed(3)} fps), ${options.seconds}s = ${sourceFrames} frames`,
  )
  log(`- Output codec: ${options.codec}, quality ${options.quality}`)
  log(`- fps = source frames encoded per wall second; CPU = encoder process CPU time / wall time (100% = one core)`)
  log()

  // Shared measurements: decoding alone, the stand-in renderer alone, and the raw pipe alone.
  const fullFrameProducer = producerArgsFor({
    width: source.width,
    height: source.height,
    fps: source.fps,
    frames: sourceFrames,
    pixFmt: 'rgba',
    box,
  })
  const shared = [
    [
      'producer alone (no pipe)',
      null,
      ['-hide_banner', '-nostats', '-loglevel', 'info', '-benchmark', ...fullFrameProducer.slice(3, -5), '-pix_fmt', 'rgba', '-f', 'null', '-'],
    ],
    [
      'decode software',
      null,
      ['-hide_banner', '-nostats', '-loglevel', 'info', '-benchmark', '-t', String(options.seconds), '-i', options.source, '-an', '-f', 'null', '-'],
    ],
    [
      'decode d3d11va/hw',
      null,
      [
        '-hide_banner',
        '-nostats',
        '-loglevel',
        'info',
        '-benchmark',
        '-hwaccel',
        process.platform === 'win32' ? 'd3d11va' : 'auto',
        '-t',
        String(options.seconds),
        '-i',
        options.source,
        '-an',
        '-f',
        'null',
        '-',
      ],
    ],
    [
      'pipe RGBA full frame only',
      fullFrameProducer,
      [
        '-hide_banner',
        '-nostats',
        '-loglevel',
        'info',
        '-benchmark',
        '-f',
        'rawvideo',
        '-pix_fmt',
        'rgba',
        '-s',
        `${source.width}x${source.height}`,
        '-r',
        fpsArg(source.fps),
        '-i',
        'pipe:0',
        '-f',
        'null',
        '-',
      ],
    ],
  ]
  const widths = [26, 9, 9, 7, 10]
  log(`## Shared stages`)
  log()
  log('```')
  log(formatRow(['stage', 'seconds', 'fps', 'CPU', 'status'], widths))
  for (const [label, producerArgs, args] of shared) {
    const result = await runPipeline({ ffmpeg: options.ffmpeg, producerArgs, encoderArgs: args })
    const cpu = result.cpuSeconds === null ? '-' : `${Math.round((100 * result.cpuSeconds) / result.seconds)}%`
    log(
      formatRow(
        [label, result.seconds.toFixed(1), (sourceFrames / result.seconds).toFixed(1), cpu, result.ok ? 'ok' : `FAILED ${result.errors}`],
        widths,
      ),
    )
  }
  log('```')

  const halfFps = { num: source.fps.num, den: source.fps.den * 2 }
  const variants = [
    { label: 'no-overlay', overlay: null },
    {
      label: 'app-today (RGBA)',
      overlay: { pixFmt: 'rgba', width: source.width, height: source.height, fps: source.fps, frames: sourceFrames, x: 0, y: 0, box },
    },
    {
      label: 'half-rate (RGBA)',
      overlay: { pixFmt: 'rgba', width: source.width, height: source.height, fps: halfFps, frames: Math.ceil(sourceFrames / 2), x: 0, y: 0, box },
    },
    {
      label: 'yuva420p pipe',
      overlay: { pixFmt: 'yuva420p', width: source.width, height: source.height, fps: source.fps, frames: sourceFrames, x: 0, y: 0, box },
      skip: ['qsvfull'],
    },
    {
      label: 'cropped RGBA box',
      overlay: {
        pixFmt: 'rgba',
        width: box.w,
        height: box.h,
        fps: source.fps,
        frames: sourceFrames,
        x: box.x,
        y: box.y,
        box: { x: 0, y: 0, w: box.w, h: box.h },
      },
    },
  ]

  for (const backend of options.backends) {
    log()
    log(`## ${backend}`)
    log()
    const unavailable = encoderAvailable(options.ffmpeg, backend, options.codec)
    if (unavailable) {
      log(`Skipped: ${unavailable}.`)
      continue
    }
    log('```')
    log(formatRow(['variant', 'seconds', 'fps', 'CPU', 'vs floor'], widths))
    let floor = null
    for (const variant of variants) {
      if (variant.skip?.includes(backend)) {
        log(formatRow([variant.label, '-', '-', '-', 'n/a for this backend'], widths))
        continue
      }
      const graph = pipeline(backend, options.codec, {
        width: source.width,
        height: source.height,
        seconds: options.seconds,
        overlay: variant.overlay ?? { x: 0, y: 0 },
      })
      const output = path.join(outDir, `${backend}-${variant.label.replace(/\W+/g, '-')}.mp4`)
      const args = encodeArgsFor({
        options,
        backend,
        source,
        filter: variant.overlay ? graph.filter : graph.noOverlay,
        overlayInput: variant.overlay,
        output,
      })
      const producerArgs = variant.overlay ? producerArgsFor({ ...variant.overlay, pixFmt: variant.overlay.pixFmt, box: variant.overlay.box }) : null
      const result = await runPipeline({ ffmpeg: options.ffmpeg, producerArgs, encoderArgs: args })
      if (!result.ok) {
        log(formatRow([variant.label, '-', '-', '-', `FAILED ${result.errors}`], widths))
        if (variant.overlay === null) break
        continue
      }
      floor ??= result.seconds
      const cpu = result.cpuSeconds === null ? '-' : `${Math.round((100 * result.cpuSeconds) / result.seconds)}%`
      log(
        formatRow(
          [
            variant.label,
            result.seconds.toFixed(1),
            (sourceFrames / result.seconds).toFixed(1),
            cpu,
            `${((100 * result.seconds) / floor).toFixed(0)}%`,
          ],
          widths,
        ),
      )
    }
    log('```')
  }

  const report = path.join(outDir, `overlay-bench-${os.hostname()}-${new Date().toISOString().replace(/[:.]/g, '-')}.md`)
  writeFileSync(report, `${lines.join('\n')}\n`)
  console.log(`\nReport: ${report}`)
}

main().catch((error) => {
  console.error(error.message)
  process.exit(1)
})
