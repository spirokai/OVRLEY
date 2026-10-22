/**
 * Bootstraps the React application and mounts the root component.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { setWorkerUrl } from 'maplibre-gl'
import '@fontsource-variable/figtree/wght.css'
import '@fontsource-variable/fira-code/wght.css'
import 'maplibre-gl/dist/maplibre-gl.css'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import './i18n/index.js'
import { hydrateLanguagePreference } from './i18n/language-preference.js'
import './index.css'
import App from './App.jsx'

// React 19.2 development builds call `performance.measure()` for every component render, and Chromium retains
// user-timing entries without a limit, so playback renders grow the WebView renderer by several MB/s. DevTools
// Performance recordings capture these measures through tracing, not this buffer, so React's tracks stay usable.
if (import.meta.env.DEV) window.setInterval(() => performance.clearMeasures(), 5_000)
setWorkerUrl(workerUrl)
const root = createRoot(document.getElementById('root'))

async function mountApp() {
  await hydrateLanguagePreference()

  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}

mountApp().catch((error) => {
  console.error('Failed to start OVRLEY:', error)
  root.render(
    <main role="alert" className="p-8 text-destructive">
      {error.message}
    </main>,
  )
})
