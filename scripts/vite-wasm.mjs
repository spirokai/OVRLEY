import { spawn } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { join, relative, resolve } from 'node:path'
import { artifactDir, artifactNames, crateDir, mapCrateDir, root, vendorDir } from './build-wasm.mjs'

/** Production assets and serialized development rebuilds of the portable renderer. */
export function wasmBindingsPlugin() {
  const buildScript = join(root, 'scripts', 'build-wasm.mjs')
  const sharedDirs = ['ovrley_widgets', 'ovrley_map'].map((name) => join(root, 'src-tauri', name))
  const widgetAssets = join(root, 'assets')
  const assets = new Map(
    artifactNames.map((name) => [
      `/${name}`,
      { file: join(artifactDir, name), type: name.endsWith('.wasm') ? 'application/wasm' : 'text/javascript' },
    ]),
  )
  assets.set('/font.ttf', { file: join(root, 'fonts', 'JetBrainsMono-variable.ttf'), type: 'font/ttf' })
  return {
    name: 'ovrley-wasm-bindings',
    async generateBundle() {
      // CI builds the host-independent bindings once and supplies them in artifactDir.
      if (process.env.OVRLEY_WASM_PREBUILT !== '1') {
        await new Promise((resolveBuild, rejectBuild) => {
          const child = spawn(process.execPath, [buildScript], { cwd: root, stdio: 'inherit' })
          child.once('error', rejectBuild)
          child.once('close', (code) => (code === 0 ? resolveBuild() : rejectBuild(new Error(`WASM build failed (${code})`))))
        })
      }
      for (const [name, asset] of assets) {
        this.emitFile({ type: 'asset', fileName: `wasm-bindings${name}`, source: await readFile(asset.file) })
      }
    },
    configureServer(server) {
      let started = false
      let pending = false
      let closed = false
      let task
      let child
      let failure
      let reloadNeeded = false

      function rebuild(reload = true) {
        reloadNeeded ||= reload
        pending = true
        if (task) return task
        task = (async () => {
          while (pending && !closed) {
            pending = false
            failure = undefined
            server.config.logger.info('[wasm] compiling bindings')
            try {
              await new Promise((resolveBuild, rejectBuild) => {
                child = spawn(process.execPath, [buildScript], { cwd: root, stdio: 'inherit' })
                child.once('error', rejectBuild)
                child.once('close', (code) =>
                  code === 0 ? resolveBuild() : rejectBuild(new Error(`Wasm build failed (exit ${code}). See terminal output.`)),
                )
              })
            } catch (error) {
              failure = error
            }
          }
          task = undefined
          if (closed) return
          if (failure) server.ws.send({ type: 'error', err: { message: failure.message, stack: '', plugin: 'ovrley-wasm-bindings' } })
          else if (reloadNeeded) server.ws.send({ type: 'full-reload', path: '*' })
          reloadNeeded = false
        })()
        return task
      }

      function sourceChanged(file) {
        const absolute = resolve(file)
        const crateRelative = relative(crateDir, absolute)
        const vendorRelative = relative(vendorDir, absolute)
        const isCrate = [crateRelative, relative(mapCrateDir, absolute)].some((path) => !path.startsWith('..')) && /\.(rs|toml|lock)$/.test(file)
        const isBundle = !vendorRelative.startsWith('..') && /(?:manifest\.json|\.tar\.gz)$/.test(file)
        const isShared = [...sharedDirs, widgetAssets].some((directory) => !relative(directory, absolute).startsWith('..'))
        if (started && (isCrate || isBundle || isShared || absolute === buildScript || absolute === join(root, '.env.local'))) rebuild()
      }

      server.watcher.add([crateDir, mapCrateDir, ...sharedDirs, widgetAssets, vendorDir, buildScript, join(root, '.env.local')])
      for (const event of ['add', 'change', 'unlink']) server.watcher.on(event, sourceChanged)
      server.httpServer?.once('close', () => {
        closed = true
        child?.kill()
        for (const event of ['add', 'change', 'unlink']) server.watcher.off(event, sourceChanged)
      })

      server.middlewares.use('/wasm-bindings', async (req, res) => {
        res.setHeader('Cache-Control', 'no-store')
        if (req.method !== 'GET' && req.method !== 'HEAD') {
          res.statusCode = 405
          res.end('Method not allowed')
          return
        }
        const asset = assets.get(new URL(req.url, 'http://localhost').pathname)
        if (!asset) {
          res.statusCode = 404
          res.end('Unknown Wasm asset')
          return
        }
        if (!started) {
          started = true
          rebuild(false)
        }
        await task
        if (failure || closed) {
          res.statusCode = 503
          res.end(failure?.message ?? 'Dev server closed')
          return
        }
        try {
          const contents = await readFile(asset.file)
          res.setHeader('Content-Type', asset.type)
          res.end(req.method === 'HEAD' ? undefined : contents)
        } catch (error) {
          res.statusCode = 503
          res.end(`Wasm artifact unavailable: ${error.message}`)
        }
      })
    },
  }
}
