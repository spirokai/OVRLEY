// Build the wasm preview with the same environment-driven approach used by
// Cyclemetry: compile Skia from source with Wasm exceptions enabled, then let
// Rust invoke Emscripten's C++ linker directly.

import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join, dirname, delimiter, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const tauriRoot = join(root, 'src-tauri')
const prebuilt = process.env.WASM_PREVIEW_PREBUILT === '1' || process.argv.includes('--prebuilt')
const artifactDir = join(tauriRoot, 'target', 'wasm32-unknown-emscripten', 'wasm-preview')
const artifacts = ['wasm_preview_poc.js', 'wasm_preview_poc.wasm']

// Release jobs consume the pair built from the same commit by the Linux Wasm
// job. They do not need an Emscripten toolchain on each native runner.
if (prebuilt) {
  if (artifacts.some(name => !existsSync(join(artifactDir, name)))) {
    console.error(`[wasm] prebuilt artifact pair is missing: ${artifactDir}`)
    process.exit(1)
  }
  console.log(`[wasm] using prebuilt artifact pair from ${artifactDir}`)
  process.exit(0)
}

const toolchainFile = join(root, '.env.wasm.local')
if (existsSync(toolchainFile)) process.loadEnvFile(toolchainFile)

const emsdk = process.env.EMSDK && resolve(root, process.env.EMSDK)
const skiaRevision = '1a80f6716b5ba787f6583443f7b62fa5f60e7084'
const toolCache = join(tauriRoot, 'target', 'wasm-preview-toolchain')

if (!emsdk) {
  console.error('[wasm] EMSDK is not set. Configure it in .env.wasm.local or activate Emscripten first.')
  process.exit(1)
}

const emscripten = join(emsdk, 'upstream', 'emscripten')
const emcc = [join(emscripten, 'emcc.exe'), join(emscripten, 'emcc.bat'), join(emscripten, 'emcc')]
  .find(existsSync)

if (!emcc) {
  console.error(`Emscripten was not found under ${emscripten}.`)
  process.exit(1)
}

// The Rust dependency is pinned, so keep a matching Skia checkout across
// ordinary app edits. An explicit source directory remains available for CI.
let skiaSource = process.env.SKIA_SOURCE_DIR && resolve(root, process.env.SKIA_SOURCE_DIR)
let sourceCheckout = null
if (!skiaSource) {
  const checkout = join(toolCache, `rust-skia-${skiaRevision}`)
  const checkoutReady = join(checkout, '.wasm-skia-checkout-ready')
  if (!existsSync(join(checkout, '.git'))) {
    if (existsSync(checkout)) throw new Error(`[wasm] incomplete rust-skia checkout: ${checkout}`)
    mkdirSync(toolCache, { recursive: true })
    console.log(`[wasm] cloning pinned rust-skia source into ${checkout}`)
    execFileSync('git', ['-c', 'core.longpaths=true', 'clone', '--filter=blob:none', '--no-checkout', 'https://github.com/rust-skia/rust-skia.git', checkout], { stdio: 'inherit' })
  }
  const currentRevision = execFileSync('git', ['-C', checkout, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
  if (currentRevision !== skiaRevision) {
    execFileSync('git', ['-C', checkout, 'fetch', '--depth=1', 'origin', skiaRevision], { stdio: 'inherit' })
    execFileSync('git', ['-c', 'core.longpaths=true', '-C', checkout, 'checkout', '--detach', skiaRevision], { stdio: 'inherit' })
  }
  if (!existsSync(checkoutReady) || !existsSync(join(checkout, 'skia-bindings', 'skia', 'DEPS'))) {
    execFileSync('git', ['-c', 'core.longpaths=true', '-C', checkout, 'submodule', 'update', '--init', '--recursive'], { stdio: 'inherit' })
    writeFileSync(checkoutReady, `${skiaRevision}\n`)
  }
  skiaSource = join(checkout, 'skia-bindings', 'skia')
  sourceCheckout = checkout
}

if (!existsSync(join(skiaSource, 'DEPS'))) {
  throw new Error(`[wasm] Skia source is incomplete: ${skiaSource}`)
}

let ninja = process.env.SKIA_NINJA_COMMAND ?? process.env.NINJA
if (!ninja) {
  try {
    execFileSync('ninja', ['--version'], { stdio: 'ignore' })
    ninja = 'ninja'
  } catch {
    if (process.platform !== 'win32') throw new Error('[wasm] Ninja is missing. Install ninja or set NINJA.')
    const ninjaDir = join(toolCache, 'ninja-1.13.1')
    ninja = join(ninjaDir, 'ninja.exe')
    if (!existsSync(ninja)) {
      mkdirSync(ninjaDir, { recursive: true })
      const archive = join(ninjaDir, 'ninja-win.zip')
      const url = 'https://github.com/ninja-build/ninja/releases/download/v1.13.1/ninja-win.zip'
      console.log(`[wasm] downloading Ninja into ${ninjaDir}`)
      const response = await fetch(url)
      if (!response.ok) throw new Error(`[wasm] Ninja download failed: HTTP ${response.status}`)
      writeFileSync(archive, Buffer.from(await response.arrayBuffer()))
      execFileSync('tar', ['-xf', archive, '-C', ninjaDir], { stdio: 'inherit' })
      if (!existsSync(ninja)) throw new Error(`[wasm] Ninja archive did not contain ninja.exe: ${archive}`)
    }
  }
}
ninja = /[\\/]/.test(ninja) ? resolve(root, ninja) : ninja
if (/[\\/]/.test(ninja) && !existsSync(ninja)) throw new Error(`[wasm] Ninja was not found: ${ninja}`)

const emxx = join(emscripten, process.platform === 'win32' ? 'em++.exe' : 'em++')
const emar = join(emscripten, process.platform === 'win32' ? 'emar.exe' : 'emar')
const compatHeaders = join(emscripten, 'system', 'include', 'compat')

// PR #1336 detects the SDK by probing a literal `emcc` path. emsdk 6 ships
// `emcc.exe` on Windows, so leave a non-executable presence marker for that
// probe and override GN with the real executable paths below.
if (process.platform === 'win32') {
  const emccProbe = join(emscripten, 'emcc')
  if (!existsSync(emccProbe)) writeFileSync(emccProbe, '')
}

const gnPath = path => path.replaceAll('\\', '/')
const env = {
  ...process.env,
  EMSDK: emsdk,
  CARGO_TARGET_DIR: join(tauriRoot, 'target'),
  SKIA_SOURCE_DIR: skiaSource,
  FORCE_SKIA_BUILD: '1',
  EMCC_CFLAGS: '-fwasm-exceptions',
  SKIA_GN_ARGS: [
    process.env.SKIA_GN_ARGS,
    `ar="${gnPath(emar)}" cc="${gnPath(emcc)}" cxx="${gnPath(emxx)}"`,
  ].filter(Boolean).join(' '),
  BINDGEN_EXTRA_CLANG_ARGS: [
    process.env.BINDGEN_EXTRA_CLANG_ARGS,
    `-isystem${compatHeaders}`,
  ].filter(Boolean).join(' '),
  PATH: [
    emscripten,
    join(emsdk, 'upstream', 'bin'),
    process.env.PATH,
  ].filter(Boolean).join(delimiter),
}

if (process.env.LIBCLANG_PATH) {
  env.LIBCLANG_PATH = resolve(root, process.env.LIBCLANG_PATH)
}

{
  const gn = join(skiaSource, 'bin', process.platform === 'win32' ? 'gn.exe' : 'gn')
  const fetchGn = join(skiaSource, 'bin', 'fetch-gn')
  const syncDeps = join(skiaSource, 'tools', 'git-sync-deps')
  const python = process.env.PYTHON ?? (process.platform === 'win32' ? 'py' : 'python3')
  const pythonArgs = process.env.PYTHON || process.platform !== 'win32' ? [] : ['-3']

  if (!existsSync(gn) && existsSync(fetchGn)) {
    execFileSync(python, [...pythonArgs, fetchGn], {
      cwd: skiaSource,
      env,
      stdio: 'inherit',
    })
  }

  const depsReady = sourceCheckout && join(sourceCheckout, '.wasm-skia-deps-ready')
  const libpngSource = join(skiaSource, 'third_party', 'externals', 'libpng', 'png.c')
  if ((!existsSync(libpngSource) || (depsReady && !existsSync(depsReady))) && existsSync(syncDeps)) {
    execFileSync(python, [...pythonArgs, syncDeps], {
      cwd: skiaSource,
      env: {
        ...env,
        GIT_SYNC_DEPS_PATH: join(skiaSource, 'DEPS'),
        GIT_SYNC_DEPS_SKIP_EMSDK: '1',
        GIT_CONFIG_COUNT: '1',
        GIT_CONFIG_KEY_0: 'core.longpaths',
        GIT_CONFIG_VALUE_0: 'true',
      },
      stdio: 'inherit',
    })
  }
  if (!existsSync(libpngSource)) throw new Error(`[wasm] Skia dependencies were not fully synced: ${libpngSource}`)
  if (depsReady && !existsSync(depsReady)) writeFileSync(depsReady, `${skiaRevision}\n`)

  env.SKIA_GN_COMMAND = process.env.SKIA_GN_COMMAND ?? gn
  env.SKIA_NINJA_COMMAND = ninja
}

// cc-rs otherwise prefers em++.bat on Windows, while emsdk 6 installs the
// executable entry points as .exe files.
if (process.platform === 'win32') {
  env.CC ??= emcc
  env.CXX ??= emxx
  env.AR ??= emar
}

const cargoArgs = [
  'build',
  '--locked',
  '-p',
  'wasm_preview_poc',
  '--target',
  'wasm32-unknown-emscripten',
  '--profile',
  'wasm-preview',
]

console.log('[wasm] building preview artifact')
execFileSync('cargo', cargoArgs, {
  cwd: join(tauriRoot, 'wasm_preview_poc'),
  env,
  stdio: 'inherit',
})
console.log('[wasm] preview artifact is up to date')
