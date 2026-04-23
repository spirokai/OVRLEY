import fs from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { finalizeParsedActivity } from '../src/api/activityParserUtils.js'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const repoRoot = path.resolve(__dirname, '..', '..')

function sanitizeDebugFilename(filename) {
  const normalizedBase = String(filename || 'activity')
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalizedBase || 'activity'}-parse-debug.json`
}

async function main() {
  const args = process.argv.slice(2)
  const rawArg = args[args.indexOf('--raw')]
  const outArg = args[args.indexOf('--out')]
  if (!rawArg || !outArg) {
    throw new Error('Usage: node generate_parse_debug_from_raw_samples.mjs --raw <raw.json> --out <debug.json>')
  }

  const rawIndex = args.indexOf('--raw')
  const outIndex = args.indexOf('--out')
  const rawPath = path.resolve(process.cwd(), args[rawIndex + 1])
  const outPath = path.resolve(process.cwd(), args[outIndex + 1])

  const payload = JSON.parse(await fs.readFile(rawPath, 'utf-8'))
  const { parsedActivity } = finalizeParsedActivity(payload)
  const debugPayload = {
    generated_at: new Date().toISOString(),
    file_name: payload.fileName,
    file_format: payload.fileFormat,
    parsed_activity: parsedActivity,
  }

  await fs.mkdir(path.dirname(outPath), { recursive: true })
  await fs.writeFile(outPath, JSON.stringify(debugPayload, null, 2), 'utf-8')
  process.stdout.write(`${outPath}\n`)
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error))
  process.exit(1)
})
