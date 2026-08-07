import { preparePackagingResources } from './packaging-resources.mjs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')

preparePackagingResources(rootDir).catch((error) => {
  console.error(`[packaging-resources] ${error.message}`)
  process.exit(1)
})
