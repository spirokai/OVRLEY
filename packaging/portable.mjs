// Packages what scripts/package.mjs built as a standalone ZIP: the binary beside the player's
// libraries, fonts, templates, map styles, FFmpeg, notices, and install guide (macOS: the .app
// bundle instead).
import { spawn, spawnSync } from 'node:child_process'
import { chmod, cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { basename, dirname, join } from 'node:path'
import { ensureDirectory, ensureFile, installGuide, noticePath, packagingDir, playerDir, preparePackagingResources, rootDir } from './resources.mjs'

const args = process.argv.slice(2)
const distDir = join(rootDir, 'dist-portable')
const appName = 'OVRLEY'
const appDir = join(distDir, appName)
const appBundleName = `${appName}.app`
const binaryName = process.platform === 'win32' ? 'OVRLEY.exe' : 'OVRLEY'
const builtBinaryPath = join(packagingDir, 'bin', binaryName)
const vendorFfmpegDir = join(rootDir, 'vendor', 'ffmpeg')
// Source directory in the checkout, and its name beside the binary.
const portableResourceDirs = [
  ['fonts', 'fonts'],
  ['templates', 'templates'],
  [join('assets', 'map-styles'), 'map-styles'],
]

main().catch((error) => {
  console.error(`[portable] ${error.message}`)
  process.exit(1)
})

async function main() {
  await preparePackagingResources()
  const version = await readArchiveVersion()
  const requestedArchiveName = readArg('--archive-name')
  const archiveName = requestedArchiveName ?? `OVRLEY-${platformSlug()}-${version}.zip`
  if (basename(archiveName) !== archiveName) {
    throw new Error(`Archive name must not contain a path: ${archiveName}`)
  }
  const archivePath = join(distDir, archiveName)

  await rm(appDir, { recursive: true, force: true })
  await mkdir(distDir, { recursive: true })

  if (process.platform === 'darwin') {
    await packageMacosApp(appDir)
  } else {
    await packagePortableBinary(appDir)
  }

  await copyPackagingDocuments(appDir)

  await rm(archivePath, { force: true })
  await zipDirectory(appDir, archivePath)
  console.log(`[portable] Created ${archivePath}`)
}

async function packagePortableBinary(destinationDir) {
  await ensureFile(builtBinaryPath, 'Staged OVRLEY binary')
  await mkdir(destinationDir, { recursive: true })

  // Windows has no rpath, so the player's DLLs lie beside the binary; Linux finds them in lib/.
  const libraryDir = process.platform === 'win32' ? destinationDir : join(destinationDir, 'lib')
  await mkdir(libraryDir, { recursive: true })
  for (const name of await readdir(playerDir)) {
    await cp(join(playerDir, name), join(libraryDir, name))
  }
  await copyPortableResourceDirs(destinationDir)
  await copyPortableFfmpeg(destinationDir)

  if (process.platform === 'linux') {
    const runtimeBinaryName = `${binaryName}-bin`
    const runtimeBinaryPath = join(destinationDir, runtimeBinaryName)
    await cp(builtBinaryPath, runtimeBinaryPath)
    await writeFile(join(destinationDir, binaryName), buildLinuxLauncher(runtimeBinaryName))
    await chmod(join(destinationDir, binaryName), 0o755)
    await chmod(runtimeBinaryPath, 0o755)
  } else {
    await cp(builtBinaryPath, join(destinationDir, binaryName))
  }

  if (process.platform === 'win32') {
    await writeFile(join(destinationDir, '.ovrley-portable'), '')
  }
}

async function copyPortableResourceDirs(destinationDir) {
  for (const [source, name] of portableResourceDirs) {
    const sourceDir = join(rootDir, source)
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
}

async function copyPackagingDocuments(destinationDir) {
  await cp(noticePath, join(destinationDir, basename(noticePath)))

  if (process.platform === 'darwin') {
    await cp(installGuide.macos, join(destinationDir, 'INSTALL.txt'))
  } else if (process.platform === 'linux') {
    await cp(installGuide.linux, join(destinationDir, 'INSTALL.txt'))
  }
}

async function packageMacosApp(destinationDir) {
  const appBundlePath = join(packagingDir, 'out', appBundleName)
  await ensureDirectory(appBundlePath, 'macOS app bundle')
  const bundledFfmpegPath = join(appBundlePath, 'Contents', 'Resources', 'vendor', 'ffmpeg', 'bin', 'ffmpeg')
  const bundledFfprobePath = join(appBundlePath, 'Contents', 'Resources', 'vendor', 'ffmpeg', 'bin', 'ffprobe')

  await ensureFile(bundledFfmpegPath, 'Bundled FFmpeg binary inside macOS app bundle')
  await ensureFile(bundledFfprobePath, 'Bundled FFprobe binary inside macOS app bundle')
  await mkdir(destinationDir, { recursive: true })
  await cp(appBundlePath, join(destinationDir, appBundleName), {
    recursive: true,
    // The framework's links are relative, and its signature seals them.
    verbatimSymlinks: true,
  })
}

function buildLinuxLauncher(runtimeBinaryName) {
  return [
    '#!/usr/bin/env sh',
    'set -eu',
    'APP_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)',
    'FFMPEG_BIN="$APP_DIR/vendor/ffmpeg/bin/ffmpeg"',
    'if [ -x "$FFMPEG_BIN" ]; then',
    '  export OVRLEY_FFMPEG="$FFMPEG_BIN"',
    'fi',
    `exec "$APP_DIR/${runtimeBinaryName}" "$@"`,
    '',
  ].join('\n')
}

function readArg(flag) {
  const index = args.indexOf(flag)
  if (index < 0) {
    return null
  }
  const value = args[index + 1]
  if (!value || value.startsWith('--')) {
    throw new Error(`Argument ${flag} requires a value`)
  }
  return value
}

function zipDirectory(sourceDir, destinationPath) {
  if (process.platform === 'win32') {
    return run('powershell', [
      '-NoProfile',
      '-Command',
      `Compress-Archive -LiteralPath '${sourceDir.replaceAll("'", "''")}' -DestinationPath '${destinationPath.replaceAll("'", "''")}' -Force`,
    ])
  }

  return run('zip', ['-qry', destinationPath, basename(sourceDir)], distDir)
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
