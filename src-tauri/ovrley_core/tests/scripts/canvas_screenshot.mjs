import { chromium } from '@playwright/test'
import { readFileSync, writeFileSync, existsSync } from 'fs'
import { resolve } from 'path'

const args = process.argv.slice(2)

function parseArg(name) {
  const idx = args.indexOf(name)
  if (idx === -1 || idx + 1 >= args.length) return null
  return args[idx + 1]
}

const mockDir = parseArg('--mock-dir')
const viteUrl = parseArg('--vite-url')
const outPath = parseArg('--out')

if (!viteUrl || !outPath) {
  console.error('Usage: node canvas_screenshot.mjs --mock-dir <path> --vite-url <url> --out <path>')
  process.exit(1)
}

async function main() {
  const browser = await chromium.launch({
    headless: true,
    args: ['--disable-font-subpixel-positioning'],
  })

  const context = await browser.newContext({
    viewport: { width: 4320, height: 2430 },
    deviceScaleFactor: 1,
  })

  const page = await context.newPage()

  await page.addInitScript(() => {
    localStorage.setItem('overlayBackgroundMode', 'transparent')
    localStorage.setItem('overlayGridVisible', 'false')
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
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
      }

      [data-testid="overlay-scene"] > div:first-child {
        background: transparent !important;
        background-color: transparent !important;
        border-color: transparent !important;
        box-shadow: none !important;
      }

      .ovrley-moveable,
      .moveable-control-box,
      .moveable-line,
      .moveable-control,
      .moveable-area,
      [data-testid="widget-badge-layer"],
      [data-testid="canvas-status-badges"] {
        display: none !important;
        opacity: 0 !important;
        visibility: hidden !important;
      }
    `,
  })

  await page.waitForFunction(() => typeof window.__STORE__ !== 'undefined', {
    timeout: 30000,
  })

  if (mockDir) {
    const storeStatePath = resolve(mockDir, 'store-state.json')
    const activityPath = resolve(mockDir, 'activity.json')
    const storeState = existsSync(storeStatePath) ? JSON.parse(readFileSync(storeStatePath, 'utf-8')) : null
    const rawActivity = existsSync(activityPath) ? JSON.parse(readFileSync(activityPath, 'utf-8')) : null
    const activity = rawActivity?.parsed_activity ?? rawActivity

    if (activity) {
      await page.evaluate((data) => {
        window.setCurrentActivityCache(data)
      }, activity)
    }

    if (storeState) {
      await page.evaluate((state) => {
        const store = window.__STORE__
        store.getState().setConfig(state.config)
        store.setState({
          globalDefaults: state.globalDefaults,
          selectedSecond: state.selectedSecond,
          activitySummary: state.activitySummary,
          selectedWidgetId: null,
          widgetDrawerOpen: false,
        })
      }, storeState)
    }
  }

  await page.waitForFunction(() => document.querySelectorAll('[data-testid="widget-layer"] [data-widget-id]').length > 0, { timeout: 15000 })

  await page.evaluate(() => document.fonts.ready)

  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)))

  // Force the overlay to render at native (unscaled) resolution.
  // The scaled scene container is a div.absolute.left-0.top-0 with inline
  // style="transform: scale(X)".  We force scale(1) so the widget layer
  // matches the config scene dimensions exactly.
  //
  // Also remove overflow-hidden on the viewport wrapper so nothing gets
  // clipped if the native-size overlay is larger than the available space.
  await page.evaluate(() => {
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'

    const scaled = document.querySelector('div.absolute.left-0\\.top-0[style*="scale"]')
    if (scaled) {
      scaled.style.transform = 'scale(1)'
      scaled.style.transformOrigin = 'left top'
    }

    // Remove overflow clipping on the viewportRef and its centering parent
    const viewportWrappers = document.querySelectorAll('.overflow-hidden')
    for (const el of viewportWrappers) {
      el.style.overflow = 'visible'
    }

    const sceneBackground = document.querySelector('[data-testid="overlay-scene"] > div:first-child')
    if (sceneBackground) {
      sceneBackground.style.background = 'transparent'
      sceneBackground.style.backgroundColor = 'transparent'
      sceneBackground.style.borderColor = 'transparent'
      sceneBackground.style.boxShadow = 'none'
    }

    const widgetLayer = document.querySelector('[data-testid="widget-layer"]')
    for (let el = widgetLayer; el; el = el.parentElement) {
      el.style.background = 'transparent'
      el.style.backgroundColor = 'transparent'
      el.style.boxShadow = 'none'
    }

    const editorChromeSelectors = [
      '.ovrley-moveable',
      '.moveable-control-box',
      '.moveable-line',
      '.moveable-control',
      '.moveable-area',
      '[data-testid="widget-badge-layer"]',
      '[data-testid="canvas-status-badges"]',
    ]
    for (const selector of editorChromeSelectors) {
      for (const el of document.querySelectorAll(selector)) {
        el.style.display = 'none'
        el.style.opacity = '0'
        el.style.visibility = 'hidden'
      }
    }
  })
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)))

  // Get the widget layer bounding box and take a screenshot
  const widgetLayer = page.locator('[data-testid="widget-layer"]').first()
  const hasWidgetLayer = await widgetLayer.count()

  if (hasWidgetLayer > 0) {
    const box = await widgetLayer.boundingBox()
    if (!box) {
      throw new Error('Widget layer exists but has no bounding box')
    }

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
      }),
    )
  } else {
    const buffer = await page.screenshot({
      fullPage: false,
      omitBackground: true,
      animations: 'disabled',
    })
    writeFileSync(resolve(outPath), buffer)
    console.log(JSON.stringify({ width: 3840, height: 2160, bg: 'transparent' }))
  }

  await browser.close()
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
