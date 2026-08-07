import { appendFile, readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const version = process.argv.slice(2)

if (version.length !== 1) {
  fail('Usage: node scripts/set-release-version.mjs X.Y.Z')
}

assertStableVersion(version[0])

await updateJsonVersion('package.json', version[0])
await updateJsonVersion('app/package.json', version[0])
await updateJsonVersion('src-tauri/tauri.conf.json', version[0])
await updateCargoManifestVersion(version[0])
await updateCargoLockVersion(version[0])

const sources = await readVersionSources()
for (const [source, value] of Object.entries(sources)) {
  if (value !== version[0]) {
    fail(`${source} contains ${JSON.stringify(value)} instead of ${version[0]}`)
  }
}

if (process.env.GITHUB_OUTPUT) {
  await appendFile(process.env.GITHUB_OUTPUT, `version=${version[0]}\n`)
}

console.log(`version=${version[0]}`)

async function updateJsonVersion(relativePath, nextVersion) {
  const path = resolve(rootDir, relativePath)
  const source = await readFile(path, 'utf8')
  const document = JSON.parse(source)

  if (!document || typeof document !== 'object' || Array.isArray(document)) {
    fail(`${relativePath} must contain a JSON object`)
  }

  document.version = nextVersion
  await writeFile(path, `${JSON.stringify(document, null, 2)}\n`)
}

async function updateCargoManifestVersion(nextVersion) {
  const path = resolve(rootDir, 'src-tauri', 'Cargo.toml')
  const source = await readFile(path, 'utf8')
  const matches = [...source.matchAll(/(\[package\][\s\S]*?\nversion\s*=\s*)"([^"]+)"/g)]

  if (matches.length !== 1) {
    fail(`Expected exactly one application package version in ${path}`)
  }

  const match = matches[0]
  const start = match.index + match[1].length
  const end = start + match[2].length + 2
  const updated = `${source.slice(0, start)}"${nextVersion}"${source.slice(end)}`
  await writeFile(path, updated)
}

async function updateCargoLockVersion(nextVersion) {
  const path = resolve(rootDir, 'src-tauri', 'Cargo.lock')
  const source = await readFile(path, 'utf8')
  const pattern = /(\[\[package\]\]\r?\nname = "app"\r?\nversion = )"([^"]+)"/g
  const matches = [...source.matchAll(pattern)]

  if (matches.length !== 1) {
    fail(`Expected exactly one application package in ${path}`)
  }

  const updated = source.replace(pattern, `$1"${nextVersion}"`)
  await writeFile(path, updated)
}

async function readVersionSources() {
  const packageJson = JSON.parse(await readFile(resolve(rootDir, 'package.json'), 'utf8'))
  const appPackageJson = JSON.parse(await readFile(resolve(rootDir, 'app/package.json'), 'utf8'))
  const tauriConfig = JSON.parse(await readFile(resolve(rootDir, 'src-tauri/tauri.conf.json'), 'utf8'))
  const cargoManifest = await readFile(resolve(rootDir, 'src-tauri/Cargo.toml'), 'utf8')
  const cargoLock = await readFile(resolve(rootDir, 'src-tauri/Cargo.lock'), 'utf8')
  const cargoManifestMatch = cargoManifest.match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)
  const cargoLockMatch = cargoLock.match(/\[\[package\]\]\r?\nname = "app"\r?\nversion = "([^"]+)"/)

  if (!cargoManifestMatch || !cargoLockMatch) {
    fail('Could not re-read the application Cargo versions')
  }

  return {
    'package.json': packageJson.version,
    'app/package.json': appPackageJson.version,
    'src-tauri/tauri.conf.json': tauriConfig.version,
    'src-tauri/Cargo.toml': cargoManifestMatch[1],
    'src-tauri/Cargo.lock': cargoLockMatch[1],
  }
}

function assertStableVersion(value) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) {
    fail(`Release version must be stable X.Y.Z SemVer: ${JSON.stringify(value)}`)
  }
}

function fail(message) {
  console.error(`[release-version] ${message}`)
  process.exit(1)
}
