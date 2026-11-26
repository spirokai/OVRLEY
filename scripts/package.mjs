// `pnpm build`: builds the app and packages it for this OS.
//   0. cargo-packager at the pinned revision, installed when cargo does not list it;
//   1. the release build of ovrley_ui;
//   2. the packaging resources (notices, the player's libraries);
//   3. the staging folder, target/packaging, with the binary under its packaged names;
//   4. cargo-packager, configured in crates/ovrley_ui/Cargo.toml: the .app on macOS, the .deb on
//      Linux, nothing on Windows unless --bundles names a format (the workflow asks for nsis);
//   5. the portable ZIP (Windows and macOS).
// --bundles <formats> picks the packager's formats and --no-bundle packages none; both leave out
// step 5. --debug packages a debug build. With CARGO_PACKAGER_SIGN_PRIVATE_KEY (and its _PASSWORD)
// set, the packager signs the updater's artifacts.
import { spawn, spawnSync } from 'node:child_process'
import { chmod, cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { installGuide, packagingDir, playerDir, preparePackagingResources, rootDir } from '../packaging/resources.mjs'

const manifestDir = join(rootDir, 'crates', 'ovrley_ui')
const binDir = join(packagingDir, 'bin')
const outDir = join(packagingDir, 'out')
const isWindows = process.platform === 'win32'
// Every package calls the binary OVRLEY, except the Windows installer: Tauri's installed app.exe,
// and updates, shortcuts and pins find the app by that name.
const packagedName = 'OVRLEY'
const installedName = isWindows ? 'app' : packagedName
const defaultFormats = { win32: [], darwin: ['app'], linux: ['deb'] }
const packagerRepository = 'https://github.com/crabnebula-dev/cargo-packager'
const packagerRev = 'd27f3df01749d64d258e0d356cb6e1bb8223c652'

main().catch((error) => {
  console.error(`[package] ${error.message}`)
  process.exit(1)
})

async function main() {
  const options = readOptions(process.argv.slice(2))
  const profile = options.debug ? 'debug' : 'release'

  // Before the app's build, so that a failed install does not wait for it; also with --no-bundle,
  // after which the AppImage workflow signs with `cargo packager signer`.
  await installPackager()
  await run('cargo', ['build', '--locked', '-p', 'ovrley_ui', '--bin', 'ovrley', ...(options.debug ? [] : ['--release'])], {
    env: { ...process.env, OVRLEY_PACKAGING: '1' },
  })
  await preparePackagingResources()
  await stageBinary(profile)

  if (options.formats.length > 0) {
    await rm(outDir, { recursive: true, force: true })
    await mkdir(outDir, { recursive: true })
    const config = await packagerConfig(options.formats)
    await run('cargo', ['packager', '--config', JSON.stringify(config)], { cwd: manifestDir })
  }
  if (options.portable) {
    await run(process.execPath, [join(rootDir, 'packaging', 'portable.mjs')])
  }
}

function readOptions(args) {
  const options = { debug: false, formats: defaultFormats[process.platform], portable: process.platform !== 'linux' }
  if (!options.formats) {
    throw new Error(`OVRLEY is packaged on Windows, macOS and Linux, not ${process.platform}`)
  }
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index]
    if (arg === '--debug') {
      options.debug = true
    } else if (arg === '--no-bundle') {
      options.formats = []
      options.portable = false
    } else if (arg === '--bundles') {
      const value = args[(index += 1)]
      if (!value || value.startsWith('--')) {
        throw new Error('Argument --bundles requires a value')
      }
      options.formats = value.split(',')
      options.portable = false
    } else {
      throw new Error(`Unknown argument ${arg}`)
    }
  }
  return options
}

// `cargo install --list` names a git install's source as `<repository>?rev=<rev>#<short>`.
async function installPackager() {
  const result = spawnSync('cargo', ['install', '--list'], { encoding: 'utf8' })
  if (result.status !== 0) {
    throw new Error(`cargo install --list failed: ${result.stderr?.trim() || result.error?.message}`)
  }
  const installed = result.stdout.split('\n').some((line) => line.startsWith('cargo-packager ') && line.includes(`rev=${packagerRev}`))
  if (installed) return
  console.log(`[package] Installing cargo-packager ${packagerRev}`)
  await run('cargo', ['install', 'cargo-packager', '--git', packagerRepository, '--rev', packagerRev, '--locked'])
}

async function stageBinary(profile) {
  const extension = isWindows ? '.exe' : ''
  await rm(binDir, { recursive: true, force: true })
  await mkdir(binDir, { recursive: true })
  for (const name of new Set([packagedName, installedName])) {
    const staged = join(binDir, `${name}${extension}`)
    await cp(join(rootDir, 'target', profile, `ovrley${extension}`), staged)
    if (!isWindows) await chmod(staged, 0o755)
  }
}

async function packagerConfig(formats) {
  const { version, config } = readPackage()
  config.version = version
  config.formats = formats
  config.binaries = [{ path: installedName, main: true }]
  config.binariesDir = binDir
  config.outDir = outDir

  if (isWindows) {
    for (const name of await readdir(playerDir)) {
      config.resources.push({ src: join(playerDir, name), target: name })
    }
  } else if (process.platform === 'darwin') {
    config.resources.push({ src: installGuide.macos, target: 'INSTALL.txt' })
    config.macos.frameworks = [join(playerDir, 'mdk.framework')]
  } else {
    config.resources.push({ src: installGuide.linux, target: 'INSTALL.txt' }, { src: playerDir, target: 'lib' })
    config.deb.files = { [await writeDebianDesktopEntry()]: 'usr/share/applications/com.ovrley.desktop.desktop' }
  }
  return config
}

function readPackage() {
  const result = spawnSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: rootDir, encoding: 'utf8', maxBuffer: 1 << 26 })
  if (result.status !== 0) {
    throw new Error(`cargo metadata failed: ${result.stderr?.trim() || result.error?.message}`)
  }
  const app = JSON.parse(result.stdout).packages.find((entry) => entry.name === 'ovrley_ui')
  return { version: app.version, config: app.metadata.packager }
}

// The AppImage's entry, with the icon name cargo-packager gives the deb's icons: the binary's.
async function writeDebianDesktopEntry() {
  const source = await readFile(join(rootDir, 'packaging', 'appimage', 'com.ovrley.desktop.desktop'), 'utf8')
  const path = join(packagingDir, 'com.ovrley.desktop.desktop')
  await writeFile(path, source.replace(/^Icon=.*$/m, `Icon=${installedName}`))
  return path
}

function run(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd: rootDir, stdio: 'inherit', ...options })
    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) resolvePromise()
      else reject(new Error(`${command} exited with ${code}`))
    })
  })
}
