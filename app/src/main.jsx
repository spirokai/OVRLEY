/**
 * Bootstraps the React application and mounts the root component.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './i18n/index.js'
import { hydrateLanguagePreference } from './i18n/language-preference.js'
import './index.css'
import App from './App.jsx'

async function mountApp() {
  await hydrateLanguagePreference()

  createRoot(document.getElementById('root')).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}

mountApp()
