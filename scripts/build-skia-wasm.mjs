// Regenerate the versioned Skia bundle. Ordinary bindings builds never run this.
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, unlinkSync, writeFileSync } from 'node:fs'
import { delimiter, join, resolve } from 'node:path'
import { archivePathFor, cargoBuild, emscriptenVersion, revision, root, rustVersion, target, toolchainEnvironment, vendorDir } from './build-wasm.mjs'

const { env, emscripten, emcc, emxx, emar } = toolchainEnvironment()
if (!env.LIBCLANG_PATH) throw new Error('Set LIBCLANG_PATH to the directory containing libclang. LLVM 23.1.0 is verified.')
env.LIBCLANG_PATH = resolve(root, env.LIBCLANG_PATH)
const cache = join(root, 'src-tauri', 'target', 'wasm-toolchain')
const checkout = join(cache, 'rust-skia')
const skia = join(checkout, 'skia-bindings', 'skia')
const run = (command, args, cwd = root) => execFileSync(command, args, { cwd, env, stdio: 'inherit' })
mkdirSync(cache, { recursive: true })
if (!existsSync(checkout))
  run('git', ['-c', 'core.longpaths=true', 'clone', '--filter=blob:none', '--no-checkout', 'https://github.com/rust-skia/rust-skia.git', checkout])
run('git', ['-C', checkout, 'fetch', '--depth=1', 'origin', revision])
run('git', ['-C', checkout, '-c', 'core.longpaths=true', 'checkout', '--detach', revision])
run('git', ['-C', checkout, '-c', 'core.longpaths=true', 'submodule', 'update', '--init', '--depth', '1'])

// skcms imports unsupported AVX/AVX-512 headers on Clang even for SSE2.
// Emscripten's immintrin.h already supplies the enabled intrinsic headers.
const intrinsicsPatch = join(root, 'scripts', 'patches', 'skcms-emscripten-intrinsics.patch')
const patchApplied = spawnSync('git', ['apply', '--reverse', '--check', intrinsicsPatch], { cwd: skia, env, stdio: 'pipe' })
if (patchApplied.error) throw patchApplied.error
if (patchApplied.status !== 0) run('git', ['apply', intrinsicsPatch], skia)

let ninja = env.NINJA
if (!ninja) {
  if (process.platform === 'win32') {
    const ninjaDir = join(cache, 'ninja-1.13.1')
    ninja = join(ninjaDir, 'ninja.exe')
    if (!existsSync(ninja)) {
      mkdirSync(ninjaDir, { recursive: true })
      const zip = join(ninjaDir, 'ninja.zip')
      run('curl.exe', ['-fL', 'https://github.com/ninja-build/ninja/releases/download/v1.13.1/ninja-win.zip', '-o', zip])
      run('tar', ['-xf', zip, '-C', ninjaDir])
    }
  } else {
    ninja = 'ninja'
  }
}
run(ninja, ['--version'])
const python = env.PYTHON ?? (process.platform === 'win32' ? 'py' : 'python3')
const pythonArgs = env.PYTHON || process.platform !== 'win32' ? [] : ['-3']
// Skia's wasm GN toolchain invokes python3, which the Windows SDK does not name.
if (process.platform === 'win32') {
  const pythonExecutable = execFileSync(python, [...pythonArgs, '-c', 'import sys; print(sys.executable)'], { env, encoding: 'utf8' }).trim()
  if (/["%\r\n]/.test(pythonExecutable)) throw new Error('Python path cannot be represented in a Windows command shim.')
  const shims = join(cache, 'bin')
  mkdirSync(shims, { recursive: true })
  writeFileSync(join(shims, 'python3.cmd'), `@"${pythonExecutable}" %*\r\n`)
  env.PATH = `${shims}${delimiter}${env.PATH}`
}
const gn = join(skia, 'bin', process.platform === 'win32' ? 'gn.exe' : 'gn')
if (!existsSync(gn)) run(python, [...pythonArgs, join(skia, 'bin', 'fetch-gn')], skia)
const depsMarker = join(cache, `deps-${revision}`)
if (!existsSync(depsMarker)) {
  env.GIT_SYNC_DEPS_SKIP_EMSDK = '1'
  env.GIT_CONFIG_COUNT = '1'
  env.GIT_CONFIG_KEY_0 = 'core.longpaths'
  env.GIT_CONFIG_VALUE_0 = 'true'
  run(python, [...pythonArgs, join(skia, 'tools', 'git-sync-deps')], skia)
  writeFileSync(depsMarker, `${revision}\n`)
}

const staging = join(cache, 'export')
const sysroot = join(emscripten, 'cache', 'sysroot').replaceAll('\\', '/')
// Emscripten translates SSE4.1 intrinsics to WASM SIMD. This Skia revision
// selects its vector raster pipeline via __SSE4_1__, not __wasm_simd128__.
const simdCflags = ['-msimd128', '-msse4.1']
Object.assign(env, {
  CARGO_TARGET_DIR: join(cache, 'target'),
  SKIA_SOURCE_DIR: skia,
  FORCE_SKIA_BUILD: '1',
  SKIA_GN_COMMAND: gn,
  SKIA_NINJA_COMMAND: ninja,
  // Skia 0.153.3 expects .bat SDK launchers on Windows; SDK 6 supplies .exe.
  // Select the installed tools explicitly, including their WASM sysroot.
  SKIA_GN_ARGS: `skia_emsdk_dir="" ar="${emar.replaceAll('\\', '/')}" cc="${emcc.replaceAll('\\', '/')}" cxx="${emxx.replaceAll('\\', '/')}" extra_cflags+=${JSON.stringify(['-fwasm-exceptions', `--sysroot=${sysroot}`, ...simdCflags])} extra_ldflags=["--sysroot=${sysroot}","-msimd128"]`,
  BINDGEN_EXTRA_CLANG_ARGS: `-isystem"${join(emscripten, 'system', 'include', 'compat')}"`,
  BUILD_ARTIFACTSTAGINGDIRECTORY: staging,
})
cargoBuild(env, ['--lib'])

const binaries = join(staging, 'skia-binaries')
for (const name of ['libskia.a', 'libskia-bindings.a', 'bindings.rs', 'LICENSE_SKIA']) {
  if (!existsSync(join(binaries, name))) throw new Error(`Skia export is incomplete: ${name}`)
}
mkdirSync(vendorDir, { recursive: true })
// Primary licenses ship as standalone files; THIRD_PARTY_NOTICES.txt below
// consolidates them with the audited notices for the whole compile graph.
cpSync(join(checkout, 'LICENSE'), join(vendorDir, 'LICENSE.rust-skia'))
cpSync(join(skia, 'LICENSE'), join(vendorDir, 'LICENSE.skia'))
// Audited against this bundle's GN compile graph: expat, freetype, libjpeg-turbo,
// libpng, wuffs and zlib. ICU, HarfBuzz, brotli and libwebp are not linked even
// though manifest features list embed-icudtl. Re-audit when changing Skia or features.
const noticeFiles = [
  ['rust-skia (MIT)', join(checkout, 'LICENSE')],
  ['Skia (BSD-3-Clause)', join(skia, 'LICENSE')],
  ['Expat (MIT)', join(skia, 'third_party/externals/expat/expat/COPYING')],
  ['FreeType (FreeType License)', join(skia, 'third_party/externals/freetype/docs/FTL.TXT')],
  ['FreeType hash support (MIT)', join(skia, 'third_party/externals/freetype/src/base/fthash.c'), 1],
  ['FreeType HarfBuzz-derived declarations', join(skia, 'third_party/externals/freetype/src/autofit/ft-hb-types.h'), 0],
  ['FreeType HarfBuzz-derived script list', join(skia, 'third_party/externals/freetype/src/autofit/hb-script-list.h'), 0],
  ['FreeType bundled zlib', join(skia, 'third_party/externals/freetype/src/gzip/zlib.h'), 0],
  ['libjpeg-turbo licenses', join(skia, 'third_party/externals/libjpeg-turbo/LICENSE.md')],
  ['Independent JPEG Group', join(skia, 'third_party/externals/libjpeg-turbo/README.ijg')],
  ['libpng', join(skia, 'third_party/externals/libpng/LICENSE')],
  ['Wuffs - Copyright 2017 The Wuffs Authors (Apache-2.0)', join(skia, 'third_party/externals/wuffs/LICENSE')],
  ['zlib', join(skia, 'third_party/externals/zlib/zlib.h'), 0],
]
const notices = noticeFiles.map(([name, file, commentIndex]) => {
  const source = readFileSync(file, 'utf8')
  const text = commentIndex === undefined ? source : [...source.matchAll(/\/\*[\s\S]*?\*\//g)][commentIndex][0]
  return `${name}\n${'='.repeat(name.length)}\n\n${text.trim()}\n`
})
writeFileSync(
  join(vendorDir, 'THIRD_PARTY_NOTICES.txt'),
  [
    'OVRLEY Skia Wasm bundle - third-party notices',
    `Source: https://github.com/rust-skia/rust-skia/tree/${revision}`,
    'Dependency revisions are pinned by that checkout and its Skia DEPS file.',
    'This software is based in part on the work of the Independent JPEG Group.',
    'Portions of this software are copyright (c) The FreeType Project (https://freetype.org). All rights reserved.',
    'FreeType is used under the FreeType License reproduced below.',
    ...notices,
  ].join('\n\n') + '\n',
)
const temporaryArchive = join(cache, 'skia-binaries.tar.gz')
run('tar', ['-czf', temporaryArchive, '-C', staging, 'skia-binaries'])
const sha256 = createHash('sha256').update(readFileSync(temporaryArchive)).digest('hex')
const archivePath = archivePathFor(sha256)
cpSync(temporaryArchive, archivePath)
writeFileSync(
  join(vendorDir, 'manifest.json'),
  `${JSON.stringify(
    {
      revision,
      rustVersion,
      emscriptenVersion,
      target,
      features: ['binary-cache', 'embed-icudtl', 'pdf', 'jpeg'],
      cflags: ['-fwasm-exceptions', ...simdCflags],
      sha256,
    },
    null,
    2,
  )}\n`,
)
// Retain only the current generated archive. The hash in its URL invalidates
// Cargo's Skia import when the bundle changes, even if the Git revision does not.
for (const name of readdirSync(vendorDir)) {
  if (/^skia-binaries(?:-[a-f0-9]{64})?\.tar\.gz$/.test(name) && join(vendorDir, name) !== archivePath) unlinkSync(join(vendorDir, name))
}
console.log(`Stored Skia bundle: ${archivePath}`)
