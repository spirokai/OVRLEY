import { appendFile, readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const version = process.argv.slice(2)

if (version.length !== 1) {
  fail('Usage: node scripts/release/set-version.mjs X.Y.Z')
}

assertStableVersion(version[0])

await updateJsonVersion('package.json', version[0])
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
  const path = resolve(rootDir, 'Cargo.toml')
  const source = await readFile(path, 'utf8')
  const matches = [...source.matchAll(/(\[workspace\.package\][^[]*?\nversion\s*=\s*)"([^"]+)"/g)]

  if (matches.length !== 1) {
    fail(`Expected exactly one workspace version in ${path}`)
  }

  const match = matches[0]
  const start = match.index + match[1].length
  const end = start + match[2].length + 2
  const updated = `${source.slice(0, start)}"${nextVersion}"${source.slice(end)}`
  await writeFile(path, updated)
}

async function updateCargoLockVersion(nextVersion) {
  const path = resolve(rootDir, 'Cargo.lock')
  let updated = await readFile(path, 'utf8')

  for (const name of await workspaceCrates()) {
    const pattern = lockEntryPattern(name)
    if (!pattern.test(updated)) {
      fail(`Expected ${name} in ${path}`)
    }
    updated = updated.replace(pattern, `$1"${nextVersion}"`)
  }

  await writeFile(path, updated)
}

// The members are crates/*, and every one inherits the workspace's version.
async function workspaceCrates() {
  const entries = await readdir(resolve(rootDir, 'crates'), { withFileTypes: true })
  return entries.filter((entry) => entry.isDirectory()).map((entry) => entry.name)
}

function lockEntryPattern(name) {
  return new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = )"([^"]+)"`)
}

async function readVersionSources() {
  const packageJson = JSON.parse(await readFile(resolve(rootDir, 'package.json'), 'utf8'))
  const cargoManifest = await readFile(resolve(rootDir, 'Cargo.toml'), 'utf8')
  const cargoLock = await readFile(resolve(rootDir, 'Cargo.lock'), 'utf8')
  const cargoManifestMatch = cargoManifest.match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/)

  if (!cargoManifestMatch) {
    fail('Could not re-read the workspace version')
  }

  const sources = { 'package.json': packageJson.version, 'Cargo.toml': cargoManifestMatch[1] }
  for (const name of await workspaceCrates()) {
    sources[`Cargo.lock (${name})`] = cargoLock.match(lockEntryPattern(name))?.[2]
  }
  return sources
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
