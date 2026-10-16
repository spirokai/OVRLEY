/**
 * Playwright screenshot script for canvas-parity tests.
 *
 * Launches a headless Chromium browser, navigates to a Vite dev server,
 * injects mock data (template, activity, store state), hides editor chrome,
 * forces the overlay to native scale, and captures a transparent-background
 * screenshot of the widget layer.
 *
 * CLI arguments:
 *   --mock-dir <path>    Directory containing mock data JSON files
 *   --vite-url <url>     URL of the running Vite dev server
 *   --out <path>         Output PNG path
 *
 * Environment:
 *   OVRLEY_CANVAS_PARITY_SHOW_EDITOR_CHROME=1  — keep editor chrome visible
 *
 * Called by the Rust test via `run_playwright_screenshot`.
 */
import { chromium } from '@playwright/test'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { artifactDir } from '../../../../scripts/build-wasm.mjs'

const args = process.argv.slice(2)

function parseArg(name) {
  const idx = args.indexOf(name)
  if (idx === -1 || idx + 1 >= args.length) return null
  return args[idx + 1]
}

const mockDir = parseArg('--mock-dir')
const viteUrl = parseArg('--vite-url')
const outPath = parseArg('--out')
const showEditorChrome = process.env.OVRLEY_CANVAS_PARITY_SHOW_EDITOR_CHROME === '1'
const hideEditorChromeCss = showEditorChrome
  ? ''
  : `
      .ovrley-moveable,
      .moveable-control-box,
      .moveable-line,
      .moveable-control,
      .moveable-area,
      [data-testid="widget-badge-layer"],
      [data-testid="canvas-status-badges"],
      [data-testid="canvas-editor-toolbar"] {
        display: none !important;
        opacity: 0 !important;
        visibility: hidden !important;
      }
    `

if (!mockDir || !viteUrl || !outPath) {
  console.error('Usage: node canvas_screenshot.mjs --mock-dir <path> --vite-url <url> --out <path>')
  process.exit(1)
}

async function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..')
  const readJson = (name) => JSON.parse(readFileSync(resolve(mockDir, name), 'utf8'))
  const template = readJson('template.json')
  const state = readJson('store-state.json')
  const activity = readJson('activity.json')
  const fonts = readJson('fonts.json')
  const errors = []
  const preferences = new Map()
  const browser = await chromium.launch({
    headless: true,
    args: ['--disable-font-subpixel-positioning'],
  })

  try {
    const context = await browser.newContext({
      viewport: { width: 4320, height: 2430 },
      deviceScaleFactor: 1,
    })

    const page = await context.newPage()
    page.on('pageerror', (error) => errors.push(error.message))
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text())
    })
    // Serve the existing build directly; Vite's normal asset route starts a build.
    await page.route('**/wasm-bindings/*', async (route) => {
      const filename = new URL(route.request().url()).pathname.split('/').at(-1)
      if (!['wasm_bindings.js', 'wasm_bindings.wasm'].includes(filename)) throw new Error(`Unexpected WASM asset: ${filename}`)
      await route.fulfill({ path: resolve(artifactDir, filename), contentType: filename.endsWith('.wasm') ? 'application/wasm' : 'text/javascript' })
    })
    await page.exposeFunction('__OVRLEY_PARITY_INVOKE__', async (command, args) => {
      switch (command) {
        case 'backend_preview_activity':
          if (args.activityId !== activity.preview_activity_id) throw new Error(`Unknown activity: ${args.activityId}`)
          return readFileSync(resolve(mockDir, 'preview-activity.json')).toString('base64')
        case 'backend_preview_font': {
          const { font, font_weight = fonts.metric_font_weight, italic = false } = args.request
          const resource = fonts.resources.find(
            ({ request }) => request.font === font && request.font_weight === font_weight && request.italic === italic,
          )
          if (!resource) throw new Error(`Missing native font resource: ${JSON.stringify(args.request)}`)
          return readFileSync(resolve(mockDir, resource.path)).toString('base64')
        }
        case 'backend_release_preview_activity':
          return null
        case 'backend_health':
          return JSON.stringify({ status: 'ok', message: 'Canvas parity fixture', ready: true })
        case 'backend_current_os':
          return { os: process.platform === 'win32' ? 'windows' : process.platform === 'darwin' ? 'macos' : 'linux' }
        case 'backend_distribution_kind':
          return 'portable'
        case 'backend_list_templates':
          return '[]'
        case 'backend_list_system_fonts':
          return JSON.stringify({ recommendedFonts: [], systemFonts: [] })
        case 'backend_detect_codecs':
          return {}
        case 'default_project_directory':
          return root
        case 'list_project_files':
          return []
        case 'plugin:store|load':
          return 1
        case 'plugin:store|get':
          return [preferences.get(args.key), preferences.has(args.key)]
        case 'plugin:store|set':
          preferences.set(args.key, args.value)
          return null
        case 'plugin:store|delete':
          return preferences.delete(args.key)
        case 'plugin:event|listen':
          return args.handler
        case 'plugin:event|unlisten':
          return null
        default:
          throw new Error(`Unhandled parity-test IPC command: ${command}`)
      }
    })
    await page.addInitScript(() => {
      const callbacks = new Map()
      let nextCallback = 0
      window.__TAURI_INTERNALS__ = {
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { label: 'main', windowLabel: 'main' },
        },
        async invoke(command, args) {
          const result = await window.__OVRLEY_PARITY_INVOKE__(command, args)
          return command === 'backend_preview_activity' || command === 'backend_preview_font'
            ? Uint8Array.from(atob(result), (character) => character.charCodeAt(0)).buffer
            : result
        },
        transformCallback(callback) {
          const id = ++nextCallback
          callbacks.set(id, callback)
          return id
        },
        unregisterCallback(id) {
          callbacks.delete(id)
        },
      }
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener(_event, id) {
          callbacks.delete(id)
        },
      }
    })

    await page.goto(viteUrl, { waitUntil: 'load', timeout: 15000 })

    await page.addStyleTag({
      content: `
      html,
      body,
      #root,
      .app-shell {
        background: transparent !important;
        background-color: transparent !important;
        zoom: 1 !important;
        --app-scale: 1 !important;
      }

      [data-testid="overlay-scene"] > div:first-child {
        background: transparent !important;
        background-color: transparent !important;
        border-color: transparent !important;
        box-shadow: none !important;
      }

      ${hideEditorChromeCss}
    `,
    })

    await page.waitForFunction(() => typeof window.__OVRLEY_STORE__ !== 'undefined', undefined, {
      timeout: 30000,
    })
    await page.getByRole('dialog').waitFor({ state: 'visible' })
    await page.keyboard.press('Escape')
    await page.getByRole('dialog').waitFor({ state: 'hidden' })

    await page.evaluate(
      async ({ template, activity, state }) => {
        const { normalizeTemplateFilePayload } = await import('/src/features/template-manager/utils/templateSnapshot.js')
        const normalized = normalizeTemplateFilePayload(template)
        const store = window.__OVRLEY_STORE__
        store.getState().setConfig(normalized.config)
        store.setState({ parsedActivity: activity, parsedActivitySource: 'activity-file', fallbackDurationSeconds: activity.trim_end_seconds })
        store.getState().setStartSecond(state.startSecond)
        store.getState().setEndSecond(state.endSecond)
        store.getState().setSelectedSecond(state.selectedSecond)
        store.setState({ globalDefaults: normalized.settings.globalDefaults, selectedWidgetId: null, selectedWidgetIds: [] })
      },
      { template, activity, state },
    )

    // The editor shell no longer hydrates background/grid prefs from browser
    // storage, so the screenshot path must force a transparent scene explicitly.
    await page.evaluate(() => {
      const scene = document.querySelector('[data-testid="overlay-scene"]')
      if (scene) {
        scene.style.background = 'transparent'
        scene.style.backgroundColor = 'transparent'
      }

      const sceneBackground = document.querySelector('[data-testid="overlay-scene"] > div:first-child')
      if (sceneBackground) {
        sceneBackground.style.background = 'transparent'
        sceneBackground.style.backgroundColor = 'transparent'
        sceneBackground.style.backgroundImage = 'none'
        sceneBackground.style.borderColor = 'transparent'
        sceneBackground.style.boxShadow = 'none'
      }
    })

    const widgetCount =
      template.config.labels.length +
      template.config.values.length +
      template.config.plots.length +
      template.config.rasters.length +
      (template.config.backdrops?.length ?? 0)
    await page.waitForFunction(
      (expected) => {
        const layer = document.querySelector('[data-testid="widget-layer"]')
        const error = layer?.querySelector('[role="alert"]')
        if (error) throw new Error(error.textContent)
        const canvases = layer?.querySelectorAll('canvas[data-wasm-widget-id]')
        const widgets = layer?.querySelectorAll('[data-widget-id]')
        return (
          canvases?.length === expected &&
          widgets.length === expected &&
          Array.from(canvases).every((canvas) => canvas.width > 0 && canvas.height > 0 && canvas.style.left !== '') &&
          Array.from(widgets).every((widget) => getComputedStyle(widget).visibility === 'visible')
        )
      },
      widgetCount,
      { timeout: 30000 },
    )

    await page.evaluate(() => document.fonts.ready)
    if (!showEditorChrome) {
      await page.addStyleTag({
        content: `
        body * { visibility: hidden !important; }
        [data-testid="widget-layer"], [data-testid="widget-layer"] * { visibility: visible !important; }
      `,
      })
    }

    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)))

    // Force the overlay to render at native (unscaled) resolution.
    // The scaled scene container is a div.absolute.left-0.top-0 with inline
    // style="transform: scale(X)".  We force scale(1) so the widget layer
    // matches the config scene dimensions exactly.
    //
    // Also remove overflow-hidden on the viewport wrapper so nothing gets
    // clipped if the native-size overlay is larger than the available space.
    await page.evaluate((shouldHideEditorChrome) => {
      document.documentElement.style.background = 'transparent'
      document.body.style.background = 'transparent'

      const scaled = document.querySelector('[data-testid="overlay-scene"]').parentElement
      Object.assign(scaled.style, {
        transform: 'scale(1)',
        transformOrigin: 'left top',
        position: 'fixed',
        left: '0',
        top: '0',
        zIndex: '2147483647',
      })

      // Remove overflow clipping on the viewportRef and its centering parent
      const viewportWrappers = document.querySelectorAll('.overflow-hidden, [data-overlay-viewport]')
      for (const el of viewportWrappers) {
        el.style.overflow = 'visible'
      }

      const sceneBackground = document.querySelector('[data-testid="overlay-scene"] > div:first-child')
      if (sceneBackground) {
        sceneBackground.style.background = 'transparent'
        sceneBackground.style.backgroundColor = 'transparent'
        sceneBackground.style.backgroundImage = 'none'
        sceneBackground.style.borderColor = 'transparent'
        sceneBackground.style.boxShadow = 'none'
      }

      const widgetLayer = document.querySelector('[data-testid="widget-layer"]')
      for (let el = widgetLayer; el; el = el.parentElement) {
        el.style.background = 'transparent'
        el.style.backgroundColor = 'transparent'
        el.style.boxShadow = 'none'
      }

      if (shouldHideEditorChrome) {
        const editorChromeSelectors = [
          '.ovrley-moveable',
          '.moveable-control-box',
          '.moveable-line',
          '.moveable-control',
          '.moveable-area',
          '[data-testid="widget-badge-layer"]',
          '[data-testid="canvas-status-badges"]',
          '[data-testid="canvas-editor-toolbar"]',
        ]
        for (const selector of editorChromeSelectors) {
          for (const el of document.querySelectorAll(selector)) {
            el.style.display = 'none'
            el.style.opacity = '0'
            el.style.visibility = 'hidden'
          }
        }
      }
    }, !showEditorChrome)
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)))

    if (showEditorChrome) {
      await page.evaluate(() => {
        const widget = document.querySelector('[data-testid="widget-layer"] [data-widget-id]')
        const widgetId = widget?.getAttribute('data-widget-id')
        if (widgetId) {
          window.__OVRLEY_STORE__?.getState?.().setSelectedWidgetId?.(widgetId)
        }
      })
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    }

    // Get the widget layer bounding box and take a screenshot
    const widgetLayer = page.locator('[data-testid="widget-layer"]').first()
    const box = await widgetLayer.boundingBox()
    if (!box) {
      throw new Error('Widget layer exists but has no bounding box')
    }
    if (box.width !== template.config.scene.width || box.height !== template.config.scene.height) {
      throw new Error(`Canvas capture is scaled: ${box.width}x${box.height}; expected ${template.config.scene.width}x${template.config.scene.height}`)
    }
    if (errors.length) throw new Error(`Browser errors:\n${errors.join('\n')}`)

    const clip = {
      x: box.x,
      y: box.y,
      width: box.width,
      height: box.height,
    }
    const buffer = await page.screenshot({
      clip,
      omitBackground: true,
      animations: 'disabled',
    })
    writeFileSync(resolve(outPath), buffer)

    console.log(
      JSON.stringify({
        width: Math.round(box.width),
        height: Math.round(box.height),
        bg: 'transparent',
        editorChrome: showEditorChrome ? 'visible' : 'hidden',
      }),
    )
  } catch (error) {
    throw new Error(`${error.message}\n${errors.join('\n')}`, { cause: error })
  } finally {
    await browser.close()
  }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
