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
const binaryPath = join(binDir, binaryName)
const requiredEncoders = process.platform === 'darwin'
  ? ['prores_ks', 'qtrle', 'prores_videotoolbox']
  : ['prores_ks', 'qtrle']
const requiredFilters = ['format', 'hwupload']

const defaultArchives = {
  win32: 'https://github.com/GyanD/codexffmpeg/releases/download/8.1.1/ffmpeg-8.1.1-full_build-shared.zip',
  darwin: 'https://evermeet.cx/ffmpeg/ffmpeg-8.1.1.zip',
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
    return
  }
  console.log(`[ffmpeg] ${existingStatus.message}`)

  const archiveUrl = process.env.OVRLEY_FFMPEG_ARCHIVE_URL ?? defaultArchives[process.platform]
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

  const discoveredBinary = await findFile(extractDir, binaryName)
  if (!discoveredBinary) {
    throw new Error(`Downloaded archive did not contain ${binaryName}`)
  }

  await rm(installDir, { recursive: true, force: true })
  await mkdir(binDir, { recursive: true })
  const discoveredBinDir = dirname(discoveredBinary)
  await cp(discoveredBinDir, binDir, { recursive: true })

  if (process.platform !== 'win32') {
    await chmod(binaryPath, 0o755)
  }

  const installedStatus = await checkFfmpeg(binaryPath)
  if (!installedStatus.usable) {
    throw new Error(`Installed ffmpeg is not usable: ${installedStatus.message}`)
  }
  console.log(`[ffmpeg] ${installedStatus.message}`)

  await rm(workDir, { recursive: true, force: true })
  console.log(`[ffmpeg] Installed ${binaryPath}`)
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

  const result = spawnSync(path, ['-version'], { encoding: 'utf8' })
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

  return {
    usable: true,
    message: `Bundled ffmpeg ${version} is current and has required features.`,
  }
}

function parseVersion(output) {
  const match = output.match(/ffmpeg version\s+(?:n)?(\d+(?:\.\d+){0,2})/i)
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
  const encoders = spawnSync(path, ['-hide_banner', '-encoders'], { encoding: 'utf8' })
  if (encoders.status !== 0) {
    return {
      usable: false,
      missing: ['encoder-list'],
    }
  }

  const filters = spawnSync(path, ['-hide_banner', '-filters'], { encoding: 'utf8' })
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
