import { spawn } from 'node:child_process'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const tauriScript = join(rootDir, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')

const args = process.argv.slice(2)
const shouldBuildPortable =
  args[0] === 'build' &&
  !args.includes('--help') &&
  !args.includes('-h') &&
  !args.includes('--bundles') &&
  !args.includes('-b') &&
  !args.includes('--no-bundle')

if (!shouldBuildPortable) {
  process.exitCode = await run(process.execPath, [tauriScript, ...args])
} else {
  const tauriBuildArgs = process.platform === 'darwin'
    ? [...args, '--bundles', 'app']
    : [...args, '--no-bundle']
  const buildCode = await run(process.execPath, [tauriScript, ...tauriBuildArgs])
  if (buildCode !== 0) {
    process.exitCode = buildCode
  } else {
    const profile = args.includes('--debug') || args.includes('-d') ? 'debug' : 'release'
    process.exitCode = await run(process.execPath, [
      join(rootDir, 'scripts', 'package-portable.mjs'),
      '--profile',
      profile,
    ])
  }
}

function run(command, commandArgs) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: rootDir,
      stdio: 'inherit',
      env: process.env,
    })
    child.on('error', reject)
    child.on('exit', (code) => resolvePromise(code ?? 1))
  })
}
