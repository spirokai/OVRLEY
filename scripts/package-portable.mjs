import { cp, mkdir, readFile, rm, stat } from 'node:fs/promises'
import { writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { spawn } from 'node:child_process'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const profile = readArg('--profile') ?? 'release'
const targetDir = join(rootDir, 'src-tauri', 'target', profile)
const distDir = join(rootDir, 'dist-portable')
const appDir = join(distDir, 'OVRLEY')
const binaryName = process.platform === 'win32' ? 'OVRLEY.exe' : 'OVRLEY'
const builtBinaryName = process.platform === 'win32' ? 'app.exe' : 'app'
const builtBinaryPath = join(targetDir, builtBinaryName)
const ffmpegBinaryPath = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg')

main().catch((error) => {
  console.error(`[portable] ${error.message}`)
  process.exit(1)
})

async function main() {
  await ensureFile(builtBinaryPath, 'Tauri binary')
  await ensureFile(ffmpegBinaryPath, 'FFmpeg binary')
  const version = await readPackageVersion()
  const archivePath = join(distDir, `OVRLEY-${platformSlug()}-${version}.zip`)

  await rm(appDir, { recursive: true, force: true })
  await mkdir(join(appDir, 'vendor'), { recursive: true })

  await cp(builtBinaryPath, join(appDir, binaryName))
  await cp(join(rootDir, 'vendor', 'ffmpeg'), join(appDir, 'vendor', 'ffmpeg'), { recursive: true })
  await cp(join(rootDir, 'fonts'), join(appDir, 'fonts'), { recursive: true })
  await cp(join(rootDir, 'templates'), join(appDir, 'templates'), { recursive: true })
  await writeFile(join(appDir, 'THIRD_PARTY_NOTICES.txt'), buildThirdPartyNotice())

  await rm(archivePath, { force: true })
  await zipDirectory(appDir, archivePath)
  console.log(`[portable] Created ${archivePath}`)
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
    'and its required runtime libraries as a separate component under vendor/ffmpeg.',
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

function readArg(flag) {
  const index = args.indexOf(flag)
  return index >= 0 ? args[index + 1] : null
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

function zipDirectory(sourceDir, destinationPath) {
  if (process.platform === 'win32') {
    return run('powershell', [
      '-NoProfile',
      '-Command',
      `Compress-Archive -LiteralPath '${sourceDir.replaceAll("'", "''")}' -DestinationPath '${destinationPath.replaceAll("'", "''")}' -Force`,
    ])
  }

  return run('zip', ['-qr', destinationPath, 'OVRLEY'], distDir)
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
