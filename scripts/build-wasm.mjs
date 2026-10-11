import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { delimiter, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadLocalEnvironment } from './build-environment.mjs'

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const crateDir = join(root, 'src-tauri', 'wasm-bindings')
export const target = 'wasm32-unknown-emscripten'
export const revision = 'b7f043e0b1e2a850e702245de811f01df1f6b0ff'
export const rustVersion = '1.98.1'
export const emscriptenVersion = '6.0.9'
export const vendorDir = join(root, 'vendor', 'skia-wasm')
export const archivePathFor = (sha256) => {
  if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error('Invalid Skia bundle SHA-256 in manifest.')
  return join(vendorDir, `skia-binaries-${sha256}.tar.gz`)
}
export const artifactDir = join(root, 'src-tauri', 'target', 'wasm-bindings', target, 'wasm')
export const artifactNames = ['wasm_bindings.js', 'wasm_bindings.wasm']

/** Load and validate the SDK boundary once for both compilation commands. */
export function toolchainEnvironment() {
  loadLocalEnvironment(root)
  if (!process.env.EMSDK) throw new Error('Set EMSDK in .env.local (see .env.example).')
  const emsdk = resolve(root, process.env.EMSDK)
  const emscripten = join(emsdk, 'upstream', 'emscripten')
  const installed = JSON.parse(readFileSync(join(emscripten, 'emscripten-version.txt'), 'utf8').trim())
  if (installed !== emscriptenVersion) throw new Error(`Expected Emscripten ${emscriptenVersion}, found ${installed}.`)
  const executable = (name) => join(emscripten, `${name}${process.platform === 'win32' ? '.exe' : ''}`)
  const emcc = executable('emcc')
  const emxx = executable('em++')
  const emar = executable('emar')
  for (const file of [emcc, emxx, emar]) {
    if (!existsSync(file)) throw new Error(`Required SDK executable missing: ${file}`)
  }
  const env = {
    ...process.env,
    EMSDK: emsdk,
    EM_CONFIG: join(emsdk, '.emscripten'),
    EMCC_CFLAGS: '-fwasm-exceptions',
    CARGO_TARGET_DIR: join(root, 'src-tauri', 'target', 'wasm-bindings'),
    CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER: emxx,
    CC_wasm32_unknown_emscripten: emcc,
    CXX_wasm32_unknown_emscripten: emxx,
    AR_wasm32_unknown_emscripten: emar,
    PATH: [process.env.PYTHON && dirname(resolve(root, process.env.PYTHON)), emscripten, join(emsdk, 'upstream', 'bin'), process.env.PATH]
      .filter(Boolean)
      .join(delimiter),
  }
  // These switches must be owned by the build mode, never inherited from a shell.
  for (const name of [
    'SKIA_SOURCE_DIR',
    'SKIA_LIBRARY_SEARCH_PATH',
    'FORCE_SKIA_BUILD',
    'FORCE_SKIA_BINARIES_DOWNLOAD',
    'SKIA_BINARIES_URL',
    'BUILD_ARTIFACTSTAGINGDIRECTORY',
    'SKIA_GN_ARGS',
  ])
    delete env[name]
  execFileSync('rustup', ['run', rustVersion, 'rustc', '--version'], { env, stdio: 'pipe' })
  return { env, emscripten, emcc, emxx, emar }
}

export function cargoBuild(env, extra = []) {
  execFileSync('cargo', [`+${rustVersion}`, 'build', '--locked', '--target', target, '--profile', 'wasm', ...extra], {
    cwd: crateDir,
    env,
    stdio: 'inherit',
  })
}

function buildWasm() {
  const manifest = JSON.parse(readFileSync(join(vendorDir, 'manifest.json'), 'utf8'))
  for (const [key, expected] of Object.entries({ revision, rustVersion, emscriptenVersion, target })) {
    if (manifest[key] !== expected) throw new Error(`Stored Skia ${key} mismatch. Rebuild with pnpm wasm:skia:build.`)
  }
  const archivePath = archivePathFor(manifest.sha256)
  const checksum = createHash('sha256').update(readFileSync(archivePath)).digest('hex')
  if (checksum !== manifest.sha256) throw new Error('Stored Skia archive checksum mismatch.')
  const { env } = toolchainEnvironment()
  // rust-skia strips file:// itself; it does not URL-decode paths or Windows drives.
  env.SKIA_BINARIES_URL = `file://${archivePath.replaceAll('\\', '/')}`
  env.FORCE_SKIA_BINARIES_DOWNLOAD = '1'
  cargoBuild(env, ['--features', 'skia-safe/no-compile'])
}

// Source compilation and Vite reuse the build contract without invoking Cargo.
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) buildWasm()
