// Writes the MapLibre style reference that `ovrley_map` validates map styles against: the `v8.json`
// of the style-spec version the frontend's maplibre-gl ships, without its documentation. Run it after
// updating maplibre-gl, so the registry accepts exactly the styles the video-sync map loads.
import { readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const referenceDir = join(rootDir, 'src-tauri', 'ovrley_map', 'style-spec')
// Documentation only: validation never reads these.
const DOCUMENTATION_KEYS = new Set(['doc', 'example', 'sdk-support', 'units', 'requires'])

const fromApp = createRequire(join(rootDir, 'app', 'package.json'))
const fromMaplibre = createRequire(fromApp.resolve('maplibre-gl/package.json'))
const specPackagePath = fromMaplibre.resolve('@maplibre/maplibre-gl-style-spec/package.json')
const specPackage = JSON.parse(await readFile(specPackagePath, 'utf8'))
const reference = JSON.parse(await readFile(join(dirname(specPackagePath), 'src', 'reference', 'v8.json'), 'utf8'))

const pruned = JSON.stringify(reference, (key, value) => (DOCUMENTATION_KEYS.has(key) ? undefined : value), 2)
await writeFile(join(referenceDir, 'v8.json'), `${pruned}\n`)
await writeFile(join(referenceDir, 'VERSION'), `${specPackage.version}\n`)
console.log(`[map-style-spec] Wrote @maplibre/maplibre-gl-style-spec ${specPackage.version} reference to ${referenceDir}`)
