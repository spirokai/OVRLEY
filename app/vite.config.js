/**
 * Configures the Vite development and build pipeline for the app.
 */

import path from 'path'
import fs from 'fs/promises'
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

// https://vite.dev/config/
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss(), parseDebugWritePlugin()],
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
