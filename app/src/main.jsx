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

setWorkerUrl(workerUrl)

async function mountApp() {
  await hydrateLanguagePreference()

  createRoot(document.getElementById('root')).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}

mountApp()
