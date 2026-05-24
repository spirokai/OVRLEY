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

  await page.waitForFunction(() => typeof window.__STORE__ !== 'undefined', { timeout: 30000 })

  if (mockDir) {
    const storeStatePath = resolve(mockDir, 'store-state.json')
    const activityPath = resolve(mockDir, 'activity.json')

    if (existsSync(storeStatePath)) {
      const storeState = JSON.parse(readFileSync(storeStatePath, 'utf-8'))
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

    if (existsSync(activityPath)) {
      const rawActivity = JSON.parse(readFileSync(activityPath, 'utf-8'))
      const activity = rawActivity.parsed_activity ?? rawActivity
      await page.evaluate((data) => {
        window.setCurrentActivityCache(data)
      }, activity)
    }
  }

  await page.waitForFunction(() => document.querySelectorAll('[data-widget-id]').length > 0, { timeout: 15000 })

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
  })
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)))

  // Get the widget layer bounding box and take a screenshot
  const widgetLayer = page.locator('div.overflow-visible').filter({ has: page.locator('[data-widget-id]') }).first()
  const hasWidgetLayer = await widgetLayer.count()

  if (hasWidgetLayer > 0) {
    const box = await widgetLayer.boundingBox()
    const buffer = await widgetLayer.screenshot()
    writeFileSync(resolve(outPath), buffer)
    console.log(JSON.stringify({
      width: Math.round(box.width),
      height: Math.round(box.height),
    }))
  } else {
    const buffer = await page.screenshot({ fullPage: false })
    writeFileSync(resolve(outPath), buffer)
    console.log(JSON.stringify({ width: 3840, height: 2160 }))
  }

  await browser.close()
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
