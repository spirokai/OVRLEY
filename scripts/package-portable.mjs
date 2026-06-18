import { chmod, cp, mkdir, readFile, readdir, rename, rm, stat } from 'node:fs/promises'
import { writeFile } from 'node:fs/promises'
import { basename, dirname, join, resolve } from 'node:path'
import { spawn } from 'node:child_process'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const profile = readArg('--profile') ?? 'release'
const targetDir = join(rootDir, 'src-tauri', 'target', profile)
const bundleDir = join(targetDir, 'bundle')
const distDir = join(rootDir, 'dist-portable')
const appName = 'OVRLEY'
const appDir = join(distDir, appName)
const appBundleName = `${appName}.app`
const binaryName = process.platform === 'win32' ? 'OVRLEY.exe' : 'OVRLEY'
const builtBinaryName = process.platform === 'win32' ? 'app.exe' : 'app'
const builtBinaryPath = join(targetDir, builtBinaryName)
const vendorFfmpegDir = join(rootDir, 'vendor', 'ffmpeg')
const portableResourceDirs = ['fonts', 'templates']
const ffmpegBinaryPath = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg')
const ffprobeBinaryPath = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe')

main().catch((error) => {
  console.error(`[portable] ${error.message}`)
  process.exit(1)
})

async function main() {
  await ensureFile(ffmpegBinaryPath, 'FFmpeg binary')
  await ensureFile(ffprobeBinaryPath, 'FFprobe binary')
  const version = await readArchiveVersion()
  const archivePath = join(distDir, `OVRLEY-${platformSlug()}-${version}.zip`)

  await rm(appDir, { recursive: true, force: true })
  await mkdir(distDir, { recursive: true })

  if (process.platform === 'darwin') {
    await packageMacosApp(appDir)
  } else {
    await packagePortableBinary(appDir)
  }

  if (process.platform === 'darwin') {
    await writeFile(join(appDir, 'README-macOS.txt'), buildMacosReadme())
  }
  if (process.platform === 'linux') {
    await writeFile(join(appDir, 'README-Linux.txt'), buildLinuxReadme())
  }

  await writeFile(join(appDir, 'THIRD_PARTY_NOTICES.txt'), buildThirdPartyNotice())

  await rm(archivePath, { force: true })
  await zipDirectory(appDir, archivePath)
  console.log(`[portable] Created ${archivePath}`)
}

async function packagePortableBinary(destinationDir) {
  await ensureFile(builtBinaryPath, 'Tauri binary')
  await mkdir(destinationDir, { recursive: true })

  const topLevelEntries = await readdir(targetDir, { withFileTypes: true })
  for (const entry of topLevelEntries) {
    if (PORTABLE_EXCLUDED_TOP_LEVEL.has(entry.name)) {
      continue
    }

    const sourcePath = join(targetDir, entry.name)
    const destinationPath = join(destinationDir, entry.name)

    if (entry.isDirectory()) {
      await cp(sourcePath, destinationPath, { recursive: true })
    } else {
      await cp(sourcePath, destinationPath)
    }
  }

  await prunePortableRuntime(destinationDir)
  await copyPortableResourceDirs(destinationDir)
  await copyPortableFfmpeg(destinationDir)

  if (process.platform === 'linux') {
    const runtimeBinaryName = `${binaryName}-bin`
    const runtimeBinaryPath = join(destinationDir, runtimeBinaryName)
    await rename(join(destinationDir, builtBinaryName), runtimeBinaryPath)
    await writeFile(join(destinationDir, binaryName), buildLinuxLauncher(runtimeBinaryName))
    await chmod(join(destinationDir, binaryName), 0o755)
    await chmod(runtimeBinaryPath, 0o755)
  } else if (binaryName !== builtBinaryName) {
    await rename(join(destinationDir, builtBinaryName), join(destinationDir, binaryName))
  }
}

async function copyPortableResourceDirs(destinationDir) {
  for (const name of portableResourceDirs) {
    const sourceDir = join(rootDir, name)
    const destinationResourceDir = join(destinationDir, name)
    await ensureDirectory(sourceDir, `Portable ${name} source directory`)
    await rm(destinationResourceDir, { recursive: true, force: true })
    await cp(sourceDir, destinationResourceDir, { recursive: true })
    await ensureDirectory(destinationResourceDir, `Packaged ${name} directory`)
  }
}

async function copyPortableFfmpeg(destinationDir) {
  const destinationFfmpegDir = join(destinationDir, 'vendor', 'ffmpeg')
  await rm(destinationFfmpegDir, { recursive: true, force: true })
  await mkdir(dirname(destinationFfmpegDir), { recursive: true })
  await cp(vendorFfmpegDir, destinationFfmpegDir, { recursive: true })

  await ensureFile(join(destinationFfmpegDir, 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'), 'Packaged FFmpeg binary')
  await ensureFile(join(destinationFfmpegDir, 'bin', process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe'), 'Packaged FFprobe binary')
  if (process.platform === 'linux') {
    await ensureDirectory(join(destinationFfmpegDir, 'lib'), 'Packaged FFmpeg shared libraries')
  }
}

async function packageMacosApp(destinationDir) {
  const appBundlePath = await resolveMacosAppBundle()
  const bundledFfmpegPath = join(appBundlePath, 'Contents', 'Resources', 'vendor', 'ffmpeg', 'bin', 'ffmpeg')
  const bundledFfprobePath = join(appBundlePath, 'Contents', 'Resources', 'vendor', 'ffmpeg', 'bin', 'ffprobe')

  await ensureFile(bundledFfmpegPath, 'Bundled FFmpeg binary inside macOS app bundle')
  await ensureFile(bundledFfprobePath, 'Bundled FFprobe binary inside macOS app bundle')
  await mkdir(destinationDir, { recursive: true })
  await cp(appBundlePath, join(destinationDir, appBundleName), { recursive: true })
}

function buildThirdPartyNotice() {
  const ffmpegVersion = spawnSync(ffmpegBinaryPath, ['-version'], {
    encoding: 'utf8',
    env: bundledFfmpegEnv(ffmpegBinaryPath),
  })
  const ffmpegLicense = spawnSync(ffmpegBinaryPath, ['-L'], {
    encoding: 'utf8',
    env: bundledFfmpegEnv(ffmpegBinaryPath),
  })
  const versionText = ffmpegVersion.status === 0 ? ffmpegVersion.stdout.trim() : 'Unable to read ffmpeg -version output.'
  const licenseText = ffmpegLicense.status === 0 ? ffmpegLicense.stdout.trim() : 'Unable to read ffmpeg -L output.'

  return [
    'THIRD-PARTY NOTICES',
    '',
    'FFmpeg',
    '-------',
    'This portable OVRLEY distribution includes unmodified FFmpeg and FFprobe command-line binaries',
    'and their required runtime libraries as separate components in the packaged resources.',
    '',
    'OVRLEY invokes ffmpeg as a subprocess for video encoding and ffprobe as a subprocess',
    'for video metadata extraction. FFmpeg and FFprobe are not linked into the OVRLEY executable.',
    '',
    'Project: https://ffmpeg.org/',
    'Source code: https://ffmpeg.org/download.html',
    'License information: https://ffmpeg.org/legal.html',
    'Upstream repository mirror: https://github.com/FFmpeg/FFmpeg',
    '',
    'Windows builds are downloaded from BtbN FFmpeg builds:',
    'https://github.com/BtbN/FFmpeg-Builds',
    '',
    'Linux builds are downloaded from BtbN FFmpeg builds:',
    'https://github.com/BtbN/FFmpeg-Builds',
    '',
    'macOS builds are downloaded from Evermeet/Tessus FFmpeg builds:',
    'https://evermeet.cx/ffmpeg/',
    '',
    'ffmpeg -version',
    '---------------',
    versionText,
    '',
    'ffmpeg -L',
    '---------',
    licenseText,
    '',
  ].join('\n')
}

function bundledFfmpegEnv(path) {
  const env = { ...process.env }
  if (process.platform === 'linux') {
    env.LD_LIBRARY_PATH = `${join(dirname(path), '..', 'lib')}:${env.LD_LIBRARY_PATH ?? ''}`
  }
  return env
}

const PORTABLE_EXCLUDED_TOP_LEVEL = new Set([
  '.fingerprint',
  'build',
  'bundle',
  'deps',
  'examples',
  'incremental',
  'nsis',
  'wix',
])

const PORTABLE_PRUNE_FILE_PATTERNS = [
  /^\.cargo-lock$/i,
  /^\.cargo-artifact/i,
  /\.d$/i,
  /\.exp$/i,
  /\.lib$/i,
  /\.pdb$/i,
  /\.rlib$/i,
]

async function prunePortableRuntime(destinationDir) {
  const entries = await readdir(destinationDir, { withFileTypes: true })

  for (const entry of entries) {
    if (!entry.isFile()) {
      continue
    }

    if (entry.name !== builtBinaryName && entry.name.toLowerCase().endsWith('.exe')) {
      await rm(join(destinationDir, entry.name), { force: true })
      continue
    }

    if (PORTABLE_PRUNE_FILE_PATTERNS.some((pattern) => pattern.test(entry.name))) {
      await rm(join(destinationDir, entry.name), { force: true })
    }
  }
}

function buildMacosReadme() {
  return [
    'OVRLEY FOR macOS',
    '',
    'Install',
    '-------',
    '1. Extract the ZIP archive.',
    '2. Move OVRLEY.app to your /Applications folder.',
    '',
    'Unsigned App Notice',
    '-------------------',
    'OVRLEY is not signed with an Apple Developer certificate, so macOS may block it from opening by default.',
    'Run this command once in Terminal after moving the app to /Applications:',
    '',
    'xattr -cr /Applications/OVRLEY.app',
    '',
  ].join('\n')
}

function buildLinuxReadme() {
  return [
    'OVRLEY FOR LINUX',
    '',
    'Install',
    '-------',
    '1. Extract the ZIP archive.',
    '2. Run ./OVRLEY from the extracted directory.',
    '',
    'Notes',
    '-----',
    'OVRLEY is packaged as a portable experimental Linux build.',
    'The OVRLEY launcher sets LD_LIBRARY_PATH so bundled FFmpeg libraries are available.',
    'Run OVRLEY-bin directly only if you provide an equivalent LD_LIBRARY_PATH yourself.',
    '',
  ].join('\n')
}

function buildLinuxLauncher(runtimeBinaryName) {
  return [
    '#!/usr/bin/env sh',
    'set -eu',
    'APP_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)',
    'FFMPEG_BIN="$APP_DIR/vendor/ffmpeg/bin/ffmpeg"',
    'FFMPEG_LIB_DIR="$APP_DIR/vendor/ffmpeg/lib"',
    'if [ -x "$FFMPEG_BIN" ]; then',
    '  export OVRLEY_FFMPEG="$FFMPEG_BIN"',
    'fi',
    'if [ -d "$FFMPEG_LIB_DIR" ]; then',
    '  export LD_LIBRARY_PATH="$FFMPEG_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"',
    'fi',
    `exec "$APP_DIR/${runtimeBinaryName}" "$@"`,
    '',
  ].join('\n')
}

function readArg(flag) {
  const index = args.indexOf(flag)
  return index >= 0 ? args[index + 1] : null
}

async function resolveMacosAppBundle() {
  const preferredPaths = [
    join(bundleDir, 'macos', appBundleName),
    join(bundleDir, 'dmg', appBundleName),
  ]

  for (const candidate of preferredPaths) {
    if (await pathIsDirectory(candidate)) {
      return candidate
    }
  }

  const discovered = await findDirectoryByName(bundleDir, appBundleName)
  if (discovered) {
    return discovered
  }

  throw new Error(`macOS app bundle not found under ${bundleDir}`)
}

async function ensureFile(path, label) {
  try {
    const entry = await stat(path)
    if (entry.isFile()) {
      return
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found at ${path}`)
}

async function ensureDirectory(path, label) {
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

async function pathIsDirectory(path) {
  try {
    const entry = await stat(path)
    return entry.isDirectory()
  } catch {
    return false
  }
}

function zipDirectory(sourceDir, destinationPath) {
  if (process.platform === 'win32') {
    return run('powershell', [
      '-NoProfile',
      '-Command',
      `Compress-Archive -LiteralPath '${sourceDir.replaceAll("'", "''")}' -DestinationPath '${destinationPath.replaceAll("'", "''")}' -Force`,
    ])
  }

  return run('zip', ['-qr', destinationPath, basename(sourceDir)], distDir)
}

function run(command, commandArgs, cwd = rootDir) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, commandArgs, { cwd, stdio: 'inherit' })
    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) resolvePromise()
      else reject(new Error(`${command} exited with ${code}`))
    })
  })
}

function platformSlug() {
  if (process.platform === 'win32') return 'win'
  if (process.platform === 'darwin') return 'macos'
  return process.platform
}

async function readPackageVersion() {
  const packageJson = JSON.parse(await readFile(join(rootDir, 'package.json'), 'utf8'))
  if (typeof packageJson.version !== 'string' || packageJson.version.length === 0) {
    throw new Error('Could not read version from package.json')
  }
  return packageJson.version
}

async function readArchiveVersion() {
  const explicitVersion = normalizeVersion(readArg('--version') ?? process.env.PORTABLE_VERSION)
  if (explicitVersion) {
    return explicitVersion
  }

  const gitTagVersion = normalizeVersion(readExactGitTag())
  if (gitTagVersion) {
    return gitTagVersion
  }

  await fetchGitTags()

  const fetchedGitTagVersion = normalizeVersion(readExactGitTag())
  if (fetchedGitTagVersion) {
    return fetchedGitTagVersion
  }

  return readPackageVersion()
}

function readExactGitTag() {
  const result = spawnSync('git', ['describe', '--tags', '--exact-match'], {
    cwd: rootDir,
    encoding: 'utf8',
  })

  if (result.status !== 0) {
    return null
  }

  return result.stdout.trim()
}

async function fetchGitTags() {
  const result = spawnSync('git', ['fetch', '--tags'], {
    cwd: rootDir,
    encoding: 'utf8',
  })

  if (result.status === 0) {
    return
  }

  const stderr = result.stderr?.trim()
  if (stderr) {
    console.warn(`[portable] Failed to fetch git tags; falling back without remote tags: ${stderr}`)
  }
}

function normalizeVersion(version) {
  if (typeof version !== 'string') {
    return null
  }

  const trimmed = version.trim()
  if (!trimmed) {
    return null
  }

  return trimmed.startsWith('v') ? trimmed.slice(1) : trimmed
}

async function findDirectoryByName(dir, targetName) {
  if (!await pathIsDirectory(dir)) {
    return null
  }

  const entries = await readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const path = join(dir, entry.name)
    if (entry.isDirectory() && entry.name === targetName) {
      return path
    }
    if (entry.isDirectory()) {
      const found = await findDirectoryByName(path, targetName)
      if (found) {
        return found
      }
    }
  }
  return null
}
