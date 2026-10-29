import { existsSync } from 'node:fs'
import { join } from 'node:path'

/** Load shared machine-specific build settings; shell variables take precedence. */
export function loadLocalEnvironment(root) {
  const local = join(root, '.env.local')
  if (existsSync(local)) process.loadEnvFile(local)
}
