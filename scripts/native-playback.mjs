// Runs the native app with video playback (the `video-playback` feature, MDK).
// MDK_SDK comes from .env.local (see .env.example); shell variables take precedence, also for
// the playback settings defaulted here (OVRLEY_PLAYBACK, OVRLEY_PLAYBACK_SIZE).
//
// `node scripts/native-playback.mjs d3d11` (Windows) renders through the D3D11 target with
// MDK's D3D11 decoder instead of D3D12.
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { delimiter, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
if (existsSync(join(rootDir, '.env.local'))) process.loadEnvFile(join(rootDir, '.env.local'))
if (!process.env.MDK_SDK) {
  throw new Error('Set MDK_SDK in .env.local to the extracted mdk-sdk folder (see .env.example).')
}
const sdk = resolve(rootDir, process.env.MDK_SDK)

const env = {
  OVRLEY_PLAYBACK: '1',
  OVRLEY_PLAYBACK_SIZE: 'canvas',
  ...(process.argv[2] === 'd3d11' ? { OVRLEY_PLAYBACK_TARGET: 'd3d11', MDK_DECODERS: 'D3D11' } : {}),
  ...process.env,
  MDK_SDK: sdk,
}
// macOS and Linux find MDK through an rpath; Windows has none, so its DLLs go on PATH.
if (process.platform === 'win32') {
  const pathKey = Object.keys(env).find((key) => key.toUpperCase() === 'PATH') ?? 'PATH'
  env[pathKey] = [join(sdk, 'bin', 'x64'), env[pathKey]].join(delimiter)
}

const cargoArgs = ['run', '--manifest-path', 'src-tauri/Cargo.toml', '-p', 'ovrley_ui', '--bin', 'ovrley', '--features', 'video-playback']
const child = spawn('cargo', cargoArgs, { cwd: rootDir, stdio: 'inherit', env })
child.on('error', (error) => {
  console.error(error)
  process.exitCode = 1
})
child.on('exit', (code) => {
  process.exitCode = code ?? 1
})
