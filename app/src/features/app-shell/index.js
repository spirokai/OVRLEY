/**
 * App shell feature — public API.
 * Application chrome: header toolbar, title bar, error handling, loading overlay, backend health.
 */

export { default as AppHeader } from './components/AppHeader'
export { default as TitleBar } from './components/TitleBar'
export { default as ErrorAlert } from './components/ErrorAlert'
export { default as LoadingOverlay } from './components/LoadingOverlay'
export { default as useBackendStatus } from './hooks/useBackendStatus'
export { hasTauriRuntime } from './utils/backendDebug'
