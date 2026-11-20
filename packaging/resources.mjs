// Checks the resources every release bundles and generates the shipped THIRD_PARTY_NOTICES.txt: the
// static notices at the repository root, then the vendored Skia notices and the bundled FFmpeg's version and
// license, which depend on the build. Runs as `pnpm prepare:packaging`; the Flatpak passes an output path.
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { mkdir, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const rootDir = resolve(import.meta.dirname, '..')
/** Where the generated notices land; tauri.conf.json and the release workflows read it here. */
export const noticePath = join(rootDir, 'src-tauri', 'target', 'packaging', 'THIRD_PARTY_NOTICES.txt')
/** Static per-platform install guides, shipped as INSTALL.txt. */
export const installGuide = {
  macos: join(import.meta.dirname, 'install', 'INSTALL-macos.txt'),
  linux: join(import.meta.dirname, 'install', 'INSTALL-linux.txt'),
  debian: join(import.meta.dirname, 'install', 'INSTALL-debian.txt'),
}

export async function preparePackagingResources(output = noticePath) {
  await ensureDirectory(join(rootDir, 'fonts'), 'Fonts source directory')
  await ensureDirectory(join(rootDir, 'templates'), 'Templates source directory')
  await ensureDirectory(join(rootDir, 'assets', 'map-styles'), 'Map styles source directory')

  const ffmpegBinary = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg')
  const ffprobeBinary = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe')
  await ensureFile(ffmpegBinary, 'FFmpeg binary')
  await ensureFile(ffprobeBinary, 'FFprobe binary')

  await mkdir(dirname(output), { recursive: true })
  await writeFile(output, buildThirdPartyNotice(ffmpegBinary))
}

function buildThirdPartyNotice(ffmpegBinaryPath) {
  return [
    readFileSync(join(rootDir, 'THIRD_PARTY_NOTICES.txt'), 'utf8'),
    '',
    readFileSync(join(rootDir, 'vendor', 'skia-wasm', 'THIRD_PARTY_NOTICES.txt'), 'utf8'),
    '',
    'ffmpeg -version',
    '---------------',
    runFfmpegCommand(ffmpegBinaryPath, ['-version']),
    '',
    'ffmpeg -L',
    '---------',
    runFfmpegCommand(ffmpegBinaryPath, ['-L']),
    '',
  ].join('\n')
}

function runFfmpegCommand(binaryPath, args) {
  const result = spawnSync(binaryPath, args, {
    encoding: 'utf8',
  })

  if (result.error || result.status !== 0) {
    throw new Error(
      `Could not read ${binaryPath} ${args.join(' ')} output: ${result.stderr?.trim() || result.error?.message || `exit ${result.status}`}`,
    )
  }

  return result.stdout.trim()
}

export async function ensureFile(path, label) {
  try {
    const entry = await stat(path)
    if (entry.isFile() && entry.size > 0) {
      return
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found or empty at ${path}`)
}

export async function ensureDirectory(path, label) {
  try {
    const entry = await stat(path)
    if (entry.isDirectory()) {
      return
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found at ${path}`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  preparePackagingResources(process.argv[2] && resolve(process.argv[2])).catch((error) => {
    console.error(`[packaging-resources] ${error.message}`)
    process.exit(1)
  })
}
