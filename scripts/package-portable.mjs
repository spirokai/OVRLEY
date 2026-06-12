import { cp, mkdir, readFile, readdir, rename, rm, stat } from 'node:fs/promises'
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
const ffmpegBinaryPath = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg')

main().catch((error) => {
  console.error(`[portable] ${error.message}`)
  process.exit(1)
})

async function main() {
  await ensureFile(ffmpegBinaryPath, 'FFmpeg binary')
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

  if (binaryName !== builtBinaryName) {
    await rename(join(destinationDir, builtBinaryName), join(destinationDir, binaryName))
  }
}

async function packageMacosApp(destinationDir) {
  const appBundlePath = await resolveMacosAppBundle()
  const bundledFfmpegPath = join(appBundlePath, 'Contents', 'Resources', 'vendor', 'ffmpeg', 'bin', 'ffmpeg')

  await ensureFile(bundledFfmpegPath, 'Bundled FFmpeg binary inside macOS app bundle')
  await mkdir(destinationDir, { recursive: true })
  await cp(appBundlePath, join(destinationDir, appBundleName), { recursive: true })
}

function buildThirdPartyNotice() {
  const ffmpegVersion = spawnSync(ffmpegBinaryPath, ['-version'], { encoding: 'utf8' })
  const ffmpegLicense = spawnSync(ffmpegBinaryPath, ['-L'], { encoding: 'utf8' })
  const versionText = ffmpegVersion.status === 0 ? ffmpegVersion.stdout.trim() : 'Unable to read ffmpeg -version output.'
  const licenseText = ffmpegLicense.status === 0 ? ffmpegLicense.stdout.trim() : 'Unable to read ffmpeg -L output.'

  return [
    'THIRD-PARTY NOTICES',
    '',
    'FFmpeg',
    '-------',
    'This portable OVRLEY distribution includes an unmodified FFmpeg command-line binary',
    'and its required runtime libraries as a separate component in the packaged resources.',
    '',
    'OVRLEY invokes ffmpeg as a subprocess for video encoding. FFmpeg is not linked into',
    'the OVRLEY executable.',
    '',
    'Project: https://ffmpeg.org/',
    'Source code: https://ffmpeg.org/download.html',
    'License information: https://ffmpeg.org/legal.html',
    'Upstream repository mirror: https://github.com/FFmpeg/FFmpeg',
    '',
    'Windows builds are downloaded from Gyan Doshi FFmpeg builds:',
    'https://www.gyan.dev/ffmpeg/builds/',
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
    '1. Open the DMG and drag OVRLEY.app to your /Applications folder.',
    '2. If you downloaded the ZIP build instead, move OVRLEY.app to /Applications manually.',
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
