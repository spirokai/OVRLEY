/**
 * Configures the Vite development and build pipeline for the app.
 */

import path from 'path'
import fs from 'fs/promises'
import { spawn } from 'node:child_process'
import process from 'node:process'
import { fileURLToPath } from 'url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

/**
 * Parses debug write plugin.
 * @returns {object} Result produced by the helper.
 */
function parseDebugWritePlugin() {
  return {
    name: 'parse-debug-write-plugin',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use('/api/parse-debug', async (req, res) => {
        if (req.method !== 'POST') {
          res.statusCode = 405
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Method not allowed' }))
          return
        }

        try {
          const body = await new Promise((resolve, reject) => {
            let rawBody = ''

            req.setEncoding('utf8')
            req.on('data', (chunk) => {
              rawBody += chunk
            })
            req.on('end', () => resolve(rawBody))
            req.on('error', reject)
          })

          const { filename, contents } = JSON.parse(body)
          if (!filename || typeof contents !== 'string') {
            res.statusCode = 400
            res.setHeader('Content-Type', 'application/json')
            res.end(JSON.stringify({ error: 'Invalid parse debug payload' }))
            return
          }

          const debugDir = path.resolve(__dirname, '..', 'debug', 'activities')
          await fs.mkdir(debugDir, { recursive: true })
          const outputPath = path.resolve(debugDir, filename)

          if (!outputPath.startsWith(debugDir)) {
            res.statusCode = 400
            res.setHeader('Content-Type', 'application/json')
            res.end(JSON.stringify({ error: 'Invalid parse debug filename' }))
            return
          }

          await fs.writeFile(outputPath, contents, 'utf8')

          res.statusCode = 200
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ path: outputPath }))
        } catch (error) {
          res.statusCode = 500
          res.setHeader('Content-Type', 'application/json')
          res.end(
            JSON.stringify({
              error: error instanceof Error ? error.message : 'Failed to write parse debug file',
            }),
          )
        }
      })
    },
  }
}

/**
 * Serves generated Wasm preview POC artifacts during local debug sessions.
 *
 * @returns {object} Result produced by the helper.
 */
function wasmPreviewArtifactPlugin() {
  const repoRoot = path.resolve(__dirname, '..')
  const artifactDir = path.join(repoRoot, 'src-tauri', 'target', 'wasm32-unknown-emscripten', 'wasm-preview')
  const pocDir = path.join(repoRoot, 'src-tauri', 'wasm_preview_poc')
  const cargoConfig = path.join(pocDir, '.cargo', 'config.toml')
  const contentTypes = {
    '.js': 'text/javascript',
    '.wasm': 'application/wasm',
  }

  return {
    name: 'wasm-preview-artifact-plugin',
    apply: 'serve',
    configureServer(server) {
      let timer
      let running = false
      let pending = false
      let closed = false
      let buildFailed = false

      function runBuild() {
        if (running || closed || !pending) return
        pending = false
        running = true
        console.log('[wasm] source changed; rebuilding preview artifact')
        const child = spawn(process.execPath, [path.join(repoRoot, 'scripts', 'build-wasm.mjs')], {
          cwd: repoRoot,
          stdio: 'inherit',
        })
        let errorMessage
        child.once('error', (error) => {
          errorMessage = error.message
        })
        child.once('close', (code) => {
          running = false
          if (closed) return
          buildFailed = code !== 0 || Boolean(errorMessage)
          if (buildFailed) {
            const message = `Wasm preview rebuild failed${errorMessage ? `: ${errorMessage}` : ` (exit ${code})`}. See terminal output.`
            console.error(`[wasm] ${message}`)
            server.ws.send({ type: 'error', err: { message, stack: '', plugin: 'wasm-preview-artifact-plugin' } })
          } else {
            server.ws.send({ type: 'full-reload' })
          }
          runBuild()
        })
      }

      function onSourceChange(file) {
        const relative = path.relative(pocDir, path.resolve(file))
        const isPocInput = relative && !relative.startsWith('..') && !path.isAbsolute(relative) && /\.(rs|toml|lock)$/.test(file)
        if (!isPocInput && path.resolve(file) !== cargoConfig) return
        clearTimeout(timer)
        timer = setTimeout(() => {
          pending = true
          runBuild()
        }, 250)
      }

      server.watcher.add([pocDir, cargoConfig])
      for (const event of ['add', 'change', 'unlink']) server.watcher.on(event, onSourceChange)
      server.httpServer?.once('close', () => {
        closed = true
        clearTimeout(timer)
      })

      server.middlewares.use('/debug/wasm-preview-artifacts', async (req, res) => {
        if (req.method !== 'GET' && req.method !== 'HEAD') {
          res.statusCode = 405
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Method not allowed' }))
          return
        }

        const requestPath = decodeURIComponent(new URL(req.url || '', 'http://localhost').pathname)
        const requestedPath = requestPath.replace(/^\/debug\/wasm-preview-artifacts\/?/, '').replace(/^\/+/, '')
        const outputPath = path.resolve(artifactDir, requestedPath)
        const isInsideArtifactDir = outputPath === artifactDir || outputPath.startsWith(`${artifactDir}${path.sep}`)

        if (!isInsideArtifactDir || !['wasm_preview_poc.js', 'wasm_preview_poc.wasm'].includes(path.basename(outputPath))) {
          res.statusCode = 404
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Unknown Wasm preview artifact' }))
          return
        }

        if (buildFailed) {
          res.statusCode = 503
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Wasm preview rebuild failed. See the dev server terminal.' }))
          return
        }

        try {
          const contents = await fs.readFile(outputPath)
          res.statusCode = 200
          res.setHeader('Content-Type', contentTypes[path.extname(outputPath)] || 'application/octet-stream')
          res.setHeader('Cache-Control', 'no-store')
          if (req.method === 'HEAD') {
            res.end()
            return
          }
          res.end(contents)
        } catch (error) {
          res.statusCode = 404
          res.setHeader('Content-Type', 'application/json')
          res.end(
            JSON.stringify({
              error: `Wasm preview artifact is missing. Run "pnpm wasm:preview:build" from the repo root. ${error.message}`,
            }),
          )
        }
      })
    },
  }
}

/**
 * Serves bundled fonts from the repo's fonts directory during local debug sessions.
 *
 * @returns {object} Result produced by the helper.
 */
function fontServingPlugin() {
  const fontsDir = path.resolve(__dirname, '..', 'fonts')
  const contentTypes = {
    '.ttf': 'font/ttf',
    '.otf': 'font/otf',
    '.ttc': 'font/ttc',
  }

  return {
    name: 'font-serving-plugin',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use('/fonts', async (req, res) => {
        if (req.method !== 'GET' && req.method !== 'HEAD') {
          res.statusCode = 405
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Method not allowed' }))
          return
        }

        const requestPath = decodeURIComponent(new URL(req.url || '', 'http://localhost').pathname)
        const requestedPath = requestPath.replace(/^\/fonts\/?/, '').replace(/^\/+/, '')
        const outputPath = path.resolve(fontsDir, requestedPath)
        const isInsideFontsDir = outputPath === fontsDir || outputPath.startsWith(`${fontsDir}${path.sep}`)

        if (!isInsideFontsDir) {
          res.statusCode = 403
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Access denied' }))
          return
        }

        const ext = path.extname(outputPath).toLowerCase()
        if (!contentTypes[ext]) {
          res.statusCode = 404
          res.setHeader('Content-Type', 'application/json')
          res.end(JSON.stringify({ error: 'Unsupported font format' }))
          return
        }

        try {
          const contents = await fs.readFile(outputPath)
          res.statusCode = 200
          res.setHeader('Content-Type', contentTypes[ext])
          res.setHeader('Cache-Control', 'public, max-age=3600')
          if (req.method === 'HEAD') {
            res.end()
            return
          }
          res.end(contents)
        } catch (error) {
          res.statusCode = 404
          res.setHeader('Content-Type', 'application/json')
          res.end(
            JSON.stringify({
              error: `Font not found: ${error.message}`,
            }),
          )
        }
      })
    },
  }
}

// https://vite.dev/config/
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss(), parseDebugWritePlugin(), wasmPreviewArtifactPlugin(), fontServingPlugin()],
  server: {
    port: 5173,
    strictPort: true,
    fs: {
      allow: [path.resolve(__dirname, '..')],
    },
  },
  build: {
    target: 'es2020',
    modulePreload: {
      polyfill: false,
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
