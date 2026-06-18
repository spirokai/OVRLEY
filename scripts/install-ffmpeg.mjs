import { createWriteStream } from 'node:fs'
import { chmod, cp, mkdir, rm, stat } from 'node:fs/promises'
import { get } from 'node:https'
import { tmpdir } from 'node:os'
import { basename, dirname, join, resolve } from 'node:path'
import { spawn, spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const MIN_VERSION = '8.1'
const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const installDir = join(rootDir, 'vendor', 'ffmpeg')
const binDir = join(installDir, 'bin')
const binaryName = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
const probeBinaryName = process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe'
const binaryPath = join(binDir, binaryName)
const probeBinaryPath = join(binDir, probeBinaryName)
const requiredEncoders = process.platform === 'darwin'
  ? ['prores_ks', 'qtrle', 'prores_videotoolbox']
  : process.platform === 'win32' || process.platform === 'linux'
    ? ['prores_ks', 'qtrle', 'h264_qsv', 'hevc_qsv']
    : ['prores_ks', 'qtrle']
const requiredFilters = process.platform === 'win32' || process.platform === 'linux'
  ? ['format', 'hwupload', 'overlay_qsv', 'hwdownload']
  : ['format', 'hwupload']

const defaultFfmpegArchives = {
  win32: 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip',
  linux: 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl-shared.tar.xz',
  darwin: 'https://evermeet.cx/ffmpeg/ffmpeg-8.1.1.zip',
}

const defaultFfprobeArchives = {
  darwin: 'https://evermeet.cx/ffmpeg/ffprobe-8.1.1.zip',
}

main().catch((error) => {
  console.error(`[ffmpeg] ${error.message}`)
  process.exit(1)
})

async function main() {
  if (process.env.OVRLEY_SKIP_FFMPEG_INSTALL === '1') {
    console.log('[ffmpeg] Skipping install because OVRLEY_SKIP_FFMPEG_INSTALL=1')
    return
  }

  const existingStatus = await checkFfmpeg(binaryPath)
  if (existingStatus.usable) {
    console.log(`[ffmpeg] ${existingStatus.message}`)
    console.log(`[ffmpeg] Using ${binaryPath}`)
    verifyInstalledTools(binaryPath, probeBinaryPath)
    return
  }
  console.log(`[ffmpeg] ${existingStatus.message}`)

  const archiveUrl = process.env.OVRLEY_FFMPEG_ARCHIVE_URL ?? defaultFfmpegArchives[process.platform]
  if (!archiveUrl) {
    console.log(`[ffmpeg] No bundled installer for ${process.platform}; install ffmpeg >= ${MIN_VERSION} on PATH or set OVRLEY_FFMPEG.`)
    return
  }

  const workDir = join(tmpdir(), `ovrley-ffmpeg-${process.pid}`)
  const archivePath = join(workDir, basename(new URL(archiveUrl).pathname))
  const extractDir = join(workDir, 'extract')

  await rm(workDir, { recursive: true, force: true })
  await mkdir(extractDir, { recursive: true })
  await mkdir(binDir, { recursive: true })

  console.log(`[ffmpeg] Downloading ${archiveUrl}`)
  await download(archiveUrl, archivePath)
  await extractArchive(archivePath, extractDir)

  const ffprobeArchiveUrl = process.env.OVRLEY_FFPROBE_ARCHIVE_URL ?? defaultFfprobeArchives[process.platform]
  if (ffprobeArchiveUrl) {
    const ffprobeArchivePath = join(workDir, basename(new URL(ffprobeArchiveUrl).pathname))
    const ffprobeExtractDir = join(workDir, 'extract-ffprobe')
    await mkdir(ffprobeExtractDir, { recursive: true })
    console.log(`[ffmpeg] Downloading ${ffprobeArchiveUrl}`)
    await download(ffprobeArchiveUrl, ffprobeArchivePath)
    await extractArchive(ffprobeArchivePath, ffprobeExtractDir)
  }

  const discoveredBinary = await findFile(extractDir, binaryName)
  if (!discoveredBinary) {
    throw new Error(`Downloaded archive did not contain ${binaryName}`)
  }

  const discoveredProbeBinary = await findFile(workDir, probeBinaryName)
  if (!discoveredProbeBinary) {
    throw new Error(`Downloaded archive did not contain ${probeBinaryName}`)
  }

  await rm(installDir, { recursive: true, force: true })
  await mkdir(binDir, { recursive: true })
  const discoveredBinDir = dirname(discoveredBinary)
  await cp(discoveredBinDir, binDir, { recursive: true })

  if (dirname(discoveredProbeBinary) !== discoveredBinDir) {
    await cp(discoveredProbeBinary, probeBinaryPath)
  }

  const discoveredLibDir = resolve(discoveredBinDir, '..', 'lib')
  let copiedLibDir = false
  try {
    await stat(discoveredLibDir)
    await cp(discoveredLibDir, join(installDir, 'lib'), { recursive: true })
    copiedLibDir = true
  } catch { /* no lib/ directory, that's fine */ }
  if (process.platform === 'linux' && !copiedLibDir) {
    throw new Error(`Downloaded Linux archive did not contain required shared libraries at ${discoveredLibDir}`)
  }

  if (process.platform !== 'win32') {
    await chmod(binaryPath, 0o755)
    await chmod(probeBinaryPath, 0o755)
  }

  const installedStatus = await checkFfmpeg(binaryPath)
  if (!installedStatus.usable) {
    throw new Error(`Installed ffmpeg is not usable: ${installedStatus.message}`)
  }
  console.log(`[ffmpeg] ${installedStatus.message}`)
  verifyInstalledTools(binaryPath, probeBinaryPath)

  await rm(workDir, { recursive: true, force: true })
  console.log(`[ffmpeg] Installed ${binaryPath}`)
  console.log(`[ffmpeg] Installed ${probeBinaryPath}`)
}

function execFfmpeg(path, args, options) {
  const env = { ...process.env }
  if (process.platform === 'linux') {
    env.LD_LIBRARY_PATH = `${join(dirname(path), '..', 'lib')}:${env.LD_LIBRARY_PATH ?? ''}`
  }
  return spawnSync(path, args, { ...options, env })
}

async function checkFfmpeg(path) {
  try {
    await stat(path)
  } catch {
    return {
      usable: false,
      message: `No bundled ffmpeg found at ${path}; downloading full build.`,
    }
  }

  const result = execFfmpeg(path, ['-version'], { encoding: 'utf8' })
  if (result.status !== 0) {
    return {
      usable: false,
      message: `Bundled ffmpeg exists at ${path}, but failed to run; downloading full build.`,
    }
  }

  const version = parseVersion(result.stdout)
  if (version === null || compareVersions(version, MIN_VERSION) < 0) {
    return {
      usable: false,
      message: `Bundled ffmpeg version ${version ?? 'unknown'} is older than ${MIN_VERSION}; downloading full build.`,
    }
  }

  const featureStatus = hasRequiredFfmpegFeatures(path)
  if (!featureStatus.usable) {
    return {
      usable: false,
      message: `Bundled ffmpeg ${version} is missing required features (${featureStatus.missing.join(', ')}); downloading full build.`,
    }
  }

  const probeStatus = checkFfprobe(join(dirname(path), probeBinaryName))
  if (!probeStatus.usable) {
    return probeStatus
  }

  return {
    usable: true,
    message: `Bundled ffmpeg ${version} is current and has required features; ffprobe is available.`,
  }
}

function checkFfprobe(path) {
  const result = execFfmpeg(path, ['-version'], { encoding: 'utf8' })
  if (result.status !== 0) {
    return {
      usable: false,
      message: `Bundled ffprobe is missing or failed to run at ${path}; downloading full build.`,
    }
  }

  return {
    usable: true,
    message: `Bundled ffprobe is available at ${path}.`,
  }
}

function verifyInstalledTools(ffmpegPath, ffprobePath) {
  const ffmpegVersion = runVersionCheck('ffmpeg', ffmpegPath)
  const ffprobeVersion = runVersionCheck('ffprobe', ffprobePath)
  console.log(`[ffmpeg] Verified ffmpeg: ${ffmpegVersion}`)
  console.log(`[ffmpeg] Verified ffmpeg path: ${ffmpegPath}`)
  console.log(`[ffmpeg] Verified ffprobe: ${ffprobeVersion}`)
  console.log(`[ffmpeg] Verified ffprobe path: ${ffprobePath}`)
}

function runVersionCheck(label, path) {
  const result = execFfmpeg(path, ['-version'], { encoding: 'utf8' })
  if (result.status !== 0) {
    const stderr = result.stderr?.trim()
    throw new Error(`${label} verification failed at ${path}${stderr ? `: ${stderr}` : ''}`)
  }

  return firstNonEmptyLine(result.stdout) ?? `${label} -version completed successfully`
}

function firstNonEmptyLine(output) {
  return output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean)
}

function parseVersion(output) {
  const match = output.match(/ffmpeg version\s+(?:n-)?(\d+(?:\.\d+){0,2})/i)
  return match?.[1] ?? null
}

function compareVersions(left, right) {
  const leftParts = left.split('.').map(Number)
  const rightParts = right.split('.').map(Number)
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const delta = (leftParts[index] ?? 0) - (rightParts[index] ?? 0)
    if (delta !== 0) return delta
  }
  return 0
}

function hasRequiredFfmpegFeatures(path) {
  const encoders = execFfmpeg(path, ['-hide_banner', '-encoders'], { encoding: 'utf8' })
  if (encoders.status !== 0) {
    return {
      usable: false,
      missing: ['encoder-list'],
    }
  }

  const filters = execFfmpeg(path, ['-hide_banner', '-filters'], { encoding: 'utf8' })
  if (filters.status !== 0) {
    return {
      usable: false,
      missing: ['filter-list'],
    }
  }

  const missingEncoders = requiredEncoders.filter((encoder) => !hasListedFeature(encoders.stdout, encoder))
  const missingFilters = requiredFilters.filter((filter) => !hasListedFeature(filters.stdout, filter))
  if (missingEncoders.length > 0 || missingFilters.length > 0) {
    return {
      usable: false,
      missing: [
        ...missingEncoders.map((encoder) => `encoder:${encoder}`),
        ...missingFilters.map((filter) => `filter:${filter}`),
      ],
    }
  }

  return {
    usable: true,
    missing: [],
  }
}

function hasListedFeature(output, feature) {
  return new RegExp(`(^|\\s)${escapeRegExp(feature)}(\\s|$)`, 'm').test(output)
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function download(url, destination) {
  return new Promise((resolvePromise, reject) => {
    const request = get(url, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode ?? 0)) {
        const location = response.headers.location
        if (!location) {
          reject(new Error(`Redirect from ${url} did not include a location`))
          return
        }
        download(new URL(location, url).toString(), destination).then(resolvePromise, reject)
        return
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download ${url}: HTTP ${response.statusCode}`))
        return
      }

      const file = createWriteStream(destination)
      const totalBytes = Number(response.headers['content-length'] ?? 0)
      let downloadedBytes = 0
      let lastProgressAt = 0

      response.on('data', (chunk) => {
        downloadedBytes += chunk.length
        const now = Date.now()
        if (now - lastProgressAt < 500) {
          return
        }
        lastProgressAt = now
        writeDownloadProgress(downloadedBytes, totalBytes)
      })

      response.pipe(file)
      file.on('finish', () => {
        writeDownloadProgress(downloadedBytes, totalBytes)
        process.stdout.write('\n')
        file.close(resolvePromise)
      })
      file.on('error', reject)
    })
    request.on('error', reject)
  })
}

function writeDownloadProgress(downloadedBytes, totalBytes) {
  const downloadedMb = bytesToMiB(downloadedBytes)
  if (totalBytes > 0) {
    const totalMb = bytesToMiB(totalBytes)
    const percent = Math.min(100, (downloadedBytes / totalBytes) * 100)
    process.stdout.write(`\r[ffmpeg] Downloaded ${downloadedMb} / ${totalMb} MiB (${percent.toFixed(1)}%)`)
    return
  }

  process.stdout.write(`\r[ffmpeg] Downloaded ${downloadedMb} MiB`)
}

function bytesToMiB(bytes) {
  return (bytes / 1024 / 1024).toFixed(1)
}

function extractArchive(archivePath, destination) {
  if (process.platform === 'win32') {
    return run('powershell', [
      '-NoProfile',
      '-Command',
      `Expand-Archive -LiteralPath '${archivePath.replaceAll("'", "''")}' -DestinationPath '${destination.replaceAll("'", "''")}' -Force`,
    ])
  }
  if (archivePath.endsWith('.tar.xz')) {
    return run('tar', ['-xf', archivePath, '-C', destination])
  }
  return run('unzip', ['-q', archivePath, '-d', destination])
}

function run(command, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { stdio: 'inherit' })
    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) resolvePromise()
      else reject(new Error(`${command} exited with ${code}`))
    })
  })
}

async function findFile(dir, targetName) {
  const { readdir } = await import('node:fs/promises')
  const entries = await readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const path = join(dir, entry.name)
    if (entry.isFile() && entry.name === targetName) {
      return path
    }
    if (entry.isDirectory()) {
      const found = await findFile(path, targetName)
      if (found) return found
    }
  }
  return null
}
