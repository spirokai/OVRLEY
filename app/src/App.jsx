import { useEffect, useState, useRef, useCallback } from 'react'
import useStore from './store/useStore'
import './index.css'
import * as backend from './api/backend'
import { applyGlobalDefaults } from './lib/config-utils'
import {
  createTemplateFilePayload,
  createTemplateState,
  downloadTemplateFile,
  normalizeTemplateFilePayload,
  sanitizeTemplateFilename,
  stringifyTemplateFile,
  templateStatesEqual,
} from './lib/template-snapshot'

// UI components
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import ControlPanel from '@/components/ControlPanel'
import ErrorAlert from '@/components/ErrorAlert'
import RenderProgressOverlay from '@/components/RenderProgressOverlay'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'

// Icons
import {
  Upload,
  Play,
  Activity,
  FolderOpen,
  Sparkles,
  Save,
} from 'lucide-react'

// Global state for sidecar
window.__SIDECAR_DEBUG__ = {
  status: 'initializing',
  error: null,
  pid: null,
  logs: [],
  startTime: null,
}

const logSidecar = (message) => {
  const timestamp = new Date().toISOString()
  console.log(`[Sidecar] ${message}`)
  window.__SIDECAR_DEBUG__.logs.push(`[${timestamp}] ${message}`)
  if (window.__SIDECAR_DEBUG__.logs.length > 50) {
    window.__SIDECAR_DEBUG__.logs.shift()
  }
}

const hasTauriRuntime = () =>
  typeof window !== 'undefined' &&
  typeof window.__TAURI_INTERNALS__ !== 'undefined'

const selectBrowserGpxFile = () =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.gpx,.fit'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

const selectBrowserTemplateFile = () =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json,application/json'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

const getFilenameFromPath = (path) => {
  const segments = String(path || '').split(/[/\\]/)
  return segments[segments.length - 1] || 'cyclemetry_template.json'
}

// Sidecar readiness monitoring
// Spinner
function Spinner({ className = 'h-4 w-4' }) {
  return (
    <svg
      className={`animate-spin ${className}`}
      viewBox="0 0 24 24"
      fill="none"
    >
      <circle
        className="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="4"
      />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  )
}

function App() {
  const {
    config,
    globalDefaults,
    setConfig,
    imageFilename,
    generatingImage,
    renderingVideo,
    setGeneratingImage,
    setImageFilename,
    setRenderProgress,
    gpxFilename,
    selectedSecond,
    setErrorMessage,
    hasUnrenderedChanges,
    setHasUnrenderedChanges,
    setLastRenderedConfig,
    autoRender,
    setAutoRender,
    templates,
    fetchTemplates,
    updateRate,
    exportRange,
    aspectRatio,
    loadedTemplateFilename,
    loadedTemplateSource,
    setLoadedTemplate,
    hydrateTemplateState,
    lastSavedTemplateState,
    setLastSavedTemplateState,
  } = useStore()

  const [backendStatus, setBackendStatus] = useState('connecting')
  const [backendReady, setBackendReady] = useState(false)
  const [imageError, setImageError] = useState(false)

  // Fetch templates once
  useEffect(() => {
    fetchTemplates()
  }, [fetchTemplates])

  // Sidecar readiness monitoring
  useEffect(() => {
    const checkInitialBackend = async () => {
      if (!hasTauriRuntime()) {
        setBackendStatus('connected')
        return
      }

      try {
        // Just check if socket exists or health check passes
        const socketExists = await backend.socketReady()
        if (socketExists) {
          await backend.healthCheck()
          setBackendStatus('connected')
        } else {
          logSidecar('Backend not yet ready, waiting for sidecar to start...')
        }
      } catch {
        logSidecar('Backend not yet reachable')
      }
    }

    checkInitialBackend()
  }, [])

  // Health polling with retry logic
  const strikesRef = useRef(0)
  useEffect(() => {
    const checkHealth = async () => {
      try {
        const health = await backend.healthCheck()
        setBackendStatus('connected')
        if (health && typeof health.ready !== 'undefined') {
          setBackendReady(health.ready)
        }
        strikesRef.current = 0
      } catch {
        strikesRef.current++
        // Be much more patient during initial connection (180 seconds)
        const threshold = backendStatus === 'connecting' ? 90 : 8
        if (strikesRef.current >= threshold && backendStatus !== 'error') {
          setBackendStatus('error')
        }
      }
    }

    const interval = setInterval(checkHealth, 2000)
    checkHealth()
    return () => clearInterval(interval)
  }, [backendStatus])

  // Template management
  const handleTemplateChange = async (filename) => {
    if (!filename) return

    try {
      setGeneratingImage(true)
      const data = await backend.getTemplate(filename)
      const templateState = createTemplateState({
        config: data,
        globalDefaults,
        updateRate,
        exportRange,
        aspectRatio,
      })

      hydrateTemplateState(templateState, {
        filename,
        source: 'backend',
      })
      setLastSavedTemplateState(templateState)

      // Auto-refresh preview with new template.
      // await handleGeneratePreview({
      //   config: data,
      //   globalDefaults: templateState.settings.globalDefaults,
      // })
    } catch (err) {
      console.error('Failed to load template:', err)
      setErrorMessage(`Failed to load template: ${err.message}`)
    } finally {
      setGeneratingImage(false)
    }
  }

  const handleSaveTemplate = async () => {
    const suggestedFilename = sanitizeTemplateFilename(
      loadedTemplateFilename || 'my_template',
    )

    try {
      const payload = createTemplateFilePayload(
        {
          config,
          globalDefaults,
          updateRate,
          exportRange,
          aspectRatio,
        },
        {
          name: suggestedFilename.replace(/\.json$/i, ''),
        },
      )
      const templateContents = stringifyTemplateFile(payload)

      if (hasTauriRuntime()) {
        const { save } = await import('@tauri-apps/plugin-dialog')
        const defaultPath =
          await backend.getDefaultTemplateSavePath(suggestedFilename)
        const selectedPath = await save({
          title: 'Save Template',
          defaultPath,
          filters: [
            {
              name: 'Cyclemetry Template',
              extensions: ['json'],
            },
          ],
        })

        if (!selectedPath) return

        await backend.writeTemplateFile(selectedPath, templateContents)

        const savedFilename = sanitizeTemplateFilename(
          getFilenameFromPath(selectedPath),
        )
        setLoadedTemplate(savedFilename, 'file')
        setLastSavedTemplateState(currentTemplateState)
        return
      }

      downloadTemplateFile(payload, suggestedFilename)
      setLoadedTemplate(suggestedFilename, 'file')
      setLastSavedTemplateState(currentTemplateState)
    } catch (err) {
      console.error('Failed to save template:', err)
      setErrorMessage(`Failed to save template: ${err.message}`)
    }
  }

  const handleImportTemplate = async () => {
    try {
      const file = await selectBrowserTemplateFile()
      if (!file) return

      const rawText = await file.text()
      const parsedTemplate = JSON.parse(rawText)
      const normalizedTemplate = normalizeTemplateFilePayload(parsedTemplate, {
        globalDefaults,
        updateRate,
        exportRange,
        aspectRatio,
      })
      const { name: _templateName, ...templateState } = normalizedTemplate
      const importedFilename = sanitizeTemplateFilename(
        normalizedTemplate.name || file.name,
      )

      hydrateTemplateState(templateState, {
        filename: importedFilename,
        source: 'file',
      })
      setLastSavedTemplateState(templateState)

      // Auto-refresh preview with imported template.
      // await handleGeneratePreview({
      //   config: templateState.config,
      //   globalDefaults: templateState.settings.globalDefaults,
      // })
    } catch (err) {
      console.error('Failed to import template:', err)
      setErrorMessage(`Failed to import template: ${err.message}`)
    }
  }

  const currentTemplateState = createTemplateState({
    config,
    globalDefaults,
    updateRate,
    exportRange,
    aspectRatio,
  })
  const status = !config
    ? null
    : !lastSavedTemplateState
      ? 'Draft'
      : templateStatesEqual(currentTemplateState, lastSavedTemplateState)
        ? 'Saved'
        : 'Modified'
  const showTemplateStatus = status === 'Draft' || status === 'Modified'

  // Generate preview
  const handleGeneratePreview = useCallback(
    async (templateOverride = null) => {
      const baseConfig = templateOverride?.config || config
      const effectiveGlobalDefaults =
        templateOverride?.globalDefaults || globalDefaults
      if (!baseConfig) return

      // Apply global defaults before sending to backend
      const currentConfig = applyGlobalDefaults(
        baseConfig,
        effectiveGlobalDefaults,
      )

      try {
        setGeneratingImage(true)
        setImageError(false)
        const data = await backend.generateDemo(
          currentConfig,
          gpxFilename || 'demo.gpxinit',
          selectedSecond,
        )

        if (data.error) {
          setErrorMessage(`Preview failed: ${data.error}`)
          setImageError(true)
        } else {
          const imageUrl = await backend.getImageUrl(data.filename)
          setImageFilename(imageUrl)
          setHasUnrenderedChanges(false)
          setLastRenderedConfig(currentConfig)
        }
      } catch (err) {
        console.error('Error generating preview:', err)
        setErrorMessage(
          `Failed to connect to backend: ${
            err.message || String(err) || 'Unknown error'
          }`,
        )
      } finally {
        setGeneratingImage(false)
      }
    },
    [
      config,
      gpxFilename,
      selectedSecond,
      globalDefaults,
      setGeneratingImage,
      setErrorMessage,
      setImageFilename,
      setHasUnrenderedChanges,
      setLastRenderedConfig,
    ],
  )

  // Render progress polling
  useEffect(() => {
    if (!renderingVideo) return

    const pollProgress = async () => {
      try {
        const data = await backend.getRenderProgress()
        setRenderProgress({
          current: data.current || 0,
          total: data.total || 0,
          status: data.status || 'rendering',
          message: data.message || '',
          estimatedSecondsRemaining: data.estimated_seconds_remaining,
        })
      } catch (err) {
        console.error('Error polling render progress:', err)
      }
    }

    const interval = setInterval(pollProgress, 500)
    pollProgress()
    return () => clearInterval(interval)
  }, [renderingVideo, setRenderProgress])

  // Auto-render effect
  useEffect(() => {
    if (
      autoRender &&
      config &&
      hasUnrenderedChanges &&
      !generatingImage &&
      backendStatus === 'connected'
    ) {
      const timer = setTimeout(() => {
        handleGeneratePreview()
      }, 1000)
      return () => clearTimeout(timer)
    }
  }, [
    config,
    globalDefaults,
    autoRender,
    hasUnrenderedChanges,
    generatingImage,
    backendStatus,
    handleGeneratePreview,
  ])

  // Load template

  // Render video
  const handleRenderVideo = async () => {
    try {
      const { default: renderVideo } = await import('./api/renderVideo')
      const result = await renderVideo()
      if (result && result.cancelled) {
        console.log('Render video cancelled (UI handled)')
        return
      }
    } catch (err) {
      console.error('Render failed:', err)
      useStore.getState().setErrorMessage(err.message || 'Unknown error')
    }
  }

  // Open downloads
  const handleOpenDownloads = async () => {
    try {
      await backend.openDownloads()
    } catch (e) {
      console.error('Error opening downloads:', e)
      setErrorMessage(`Failed to open downloads folder: ${e.message}`)
    }
  }

  // Handle GPX file selection
  const handleGpxFileOpen = async () => {
    try {
      const { default: saveFileFromPath } = await import('./api/gpxUtils')
      const selected = await selectBrowserGpxFile()

      if (!selected) return

      setGeneratingImage(true)
      setImageError(false)

      await saveFileFromPath(selected)

      // Refresh preview after gpx load
      await handleGeneratePreview()
    } catch (err) {
      console.error('GPX selection failed:', err)
      useStore
        .getState()
        .setErrorMessage(`GPX Selection failed: ${err.message}`)
    } finally {
      setGeneratingImage(false)
    }
  }

  return (
    <div className="h-screen flex flex-col bg-background text-foreground">
      <ErrorAlert />
      <RenderProgressOverlay />
      {/* Header */}
      <header className="relative z-50 shrink-0 border-b border-border/70 bg-card/80 backdrop-blur-sm">
        <div className="flex items-center justify-between px-6 py-3">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-3">
              <img
                src="/logo192.png"
                alt="Cyclemetry"
                className="w-8 h-8 rounded-lg"
              />
              <div className="hidden sm:block">
                <h1 className="font-semibold text-sm">Cyclemetry</h1>
                <p className="text-[10px] text-muted-foreground">
                  Overlay Editor
                </p>
              </div>
            </div>

            <div className="h-8 w-px bg-border/60" />

            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Button
                  className="mr-4 h-9 gap-2 border-border/70 px-5 "
                  onClick={handleGpxFileOpen}
                >
                  <Activity className="h-3.5 w-3.5" />
                  <span className="max-w-28 truncate">
                    {gpxFilename === 'demo.gpxinit'
                      ? 'Load GPX/FIT'
                      : gpxFilename || 'Load GPX/FIT'}
                  </span>
                </Button>

                <Select
                  value={
                    loadedTemplateSource === 'backend'
                      ? loadedTemplateFilename || ''
                      : ''
                  }
                  onValueChange={handleTemplateChange}
                >
                  <SelectTrigger className="h-8 w-56 bg-surface text-xs border-border/70">
                    <div className="flex items-center gap-2 truncate">
                      <Sparkles className="h-3 w-3 shrink-0 text-primary" />
                      <SelectValue
                        placeholder={
                          loadedTemplateSource === 'file'
                            ? loadedTemplateFilename || 'Imported Template'
                            : 'Select Template...'
                        }
                      />
                    </div>
                  </SelectTrigger>
                  <SelectContent>
                    {templates.map((t) => (
                      <SelectItem key={t.id} value={t.id}>
                        {t.name} {t.type === 'user' && '(User)'}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>

                <div className="flex items-center gap-1">
                  {showTemplateStatus && config && (
                    <SimpleTooltip side="bottom" content="Save Template">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                        onClick={handleSaveTemplate}
                      >
                        <Save className="h-4 w-4" />
                      </Button>
                    </SimpleTooltip>
                  )}
                  <SimpleTooltip side="bottom" content="Import Template JSON">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                      onClick={handleImportTemplate}
                    >
                      <FolderOpen className="h-4 w-4" />
                    </Button>
                  </SimpleTooltip>
                </div>

                {showTemplateStatus ? (
                  <Badge
                    variant={status === 'Modified' ? 'secondary' : 'outline'}
                    className={`text-[10px] h-5 ${
                      status === 'Modified'
                        ? 'border-accent-border bg-surface-accent-soft text-primary'
                        : ''
                    }`}
                  >
                    {status}
                  </Badge>
                ) : null}
              </div>
            </div>
          </div>

          {/* Right - Actions & Status */}
          <div className="flex items-center gap-3">
            <SimpleTooltip
              side="bottom"
              content={
                !config
                  ? 'Load a template or GPX first'
                  : backendStatus !== 'connected'
                    ? 'Backend offline'
                    : !hasUnrenderedChanges
                      ? 'No changes to render'
                      : generatingImage
                        ? 'Generating preview...'
                        : null
              }
            >
              <Button
                variant="outline"
                size="sm"
                className={`gap-2 h-8 px-3 transition-all duration-300 relative ${
                  hasUnrenderedChanges
                    ? 'border-accent-border bg-surface-accent-soft text-foreground ring-1 ring-ring/50'
                    : 'border-accent-border/70 hover:border-accent-border hover:bg-surface-accent-soft'
                }`}
                onClick={() => handleGeneratePreview()}
                disabled={
                  generatingImage || !config || backendStatus !== 'connected'
                }
              >
                {generatingImage ? (
                  <Spinner className="h-3.5 w-3.5" />
                ) : (
                  <Upload
                    className={`h-3.5 w-3.5 ${hasUnrenderedChanges ? 'text-highlight' : 'text-primary'}`}
                  />
                )}
                <span>Refresh Preview</span>
                {hasUnrenderedChanges && !generatingImage && (
                  <span className="absolute -top-1 -right-1 flex h-2.5 w-2.5">
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-highlight opacity-75"></span>
                    <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-primary"></span>
                  </span>
                )}
              </Button>
            </SimpleTooltip>

            <div className="flex items-center gap-2 mr-1">
              <Switch
                id="auto-render"
                checked={autoRender}
                onCheckedChange={setAutoRender}
              />
              <Label
                htmlFor="auto-render"
                className="text-xs text-muted-foreground whitespace-nowrap cursor-pointer"
              >
                Auto
              </Label>
            </div>

            <SimpleTooltip
              side="bottom"
              content={
                !config
                  ? 'Load a template first'
                  : backendStatus !== 'connected'
                    ? 'Backend offline'
                    : renderingVideo
                      ? 'Rendering already in progress'
                      : null
              }
            >
              <Button
                size="sm"
                className="bg-primary text-primary-foreground hover:bg-primary/90"
                disabled={
                  !config || renderingVideo || backendStatus !== 'connected'
                }
                onClick={handleRenderVideo}
              >
                <Play className="mr-2 h-4 w-4" />
                {renderingVideo ? 'Rendering...' : 'Render'}
              </Button>
            </SimpleTooltip>

            <SimpleTooltip
              side="bottom"
              content={backendStatus !== 'connected' ? 'Backend offline' : null}
            >
              <Button
                variant="outline"
                size="sm"
                className="h-8 gap-2 border-accent-border/70 px-3 text-muted-foreground hover:border-accent-border hover:bg-surface-accent-soft hover:text-foreground"
                disabled={backendStatus !== 'connected'}
                onClick={handleOpenDownloads}
              >
                <FolderOpen className="h-3.5 w-3.5" />
                <span>Downloads</span>
              </Button>
            </SimpleTooltip>

            <div className="h-6 w-px bg-border" />

            {backendStatus === 'connected' && !backendReady && (
              <Badge
                variant="secondary"
                className="gap-1.5 transition-all duration-300"
              >
                <Spinner className="h-3 w-3" />
                <span>Loading Libs...</span>
              </Badge>
            )}

            <div className="flex items-center gap-2">
              {backendStatus === 'connecting' && <Spinner />}
              <Badge
                variant={
                  backendStatus === 'connected'
                    ? 'default'
                    : backendStatus === 'connecting'
                      ? 'secondary'
                      : 'destructive'
                }
              >
                {backendStatus === 'connected'
                  ? 'Connected'
                  : backendStatus === 'connecting'
                    ? 'Starting...'
                    : 'Offline'}
              </Badge>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Preview - Left */}
        <div className="flex flex-1 items-center justify-center bg-background p-8">
          {generatingImage && (
            <div className="absolute inset-0 bg-background/80 flex items-center justify-center z-10">
              <div className="flex flex-col items-center gap-2">
                <Spinner className="h-8 w-8" />
                <span className="text-sm text-muted-foreground">
                  Generating preview...
                </span>
              </div>
            </div>
          )}

          {imageFilename && !imageError ? (
            <div className="relative">
              <img
                src={imageFilename}
                alt="Preview"
                className="max-h-full max-w-full rounded-lg border border-border/70 bg-grid-transparent object-contain shadow-2xl"
                onError={() => setImageError(true)}
              />
              {config?.scene && (
                <div className="absolute top-4 left-1/2 -translate-x-1/2">
                  <div className="rounded-full border border-border/70 bg-surface-overlay px-3 py-1 backdrop-blur-sm">
                    <span className="text-xs font-medium text-muted-foreground">
                      {config.scene.width} × {config.scene.height}
                    </span>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="flex flex-col items-center gap-4 text-center">
              {backendStatus === 'connecting' ? (
                <>
                  <div className="flex h-16 w-16 items-center justify-center rounded-full bg-surface-elevated">
                    <Spinner className="h-6 w-6" />
                  </div>
                  <div>
                    <p className="font-medium">
                      {strikesRef.current > 5
                        ? 'Still starting up...'
                        : 'Starting Backend'}
                    </p>
                    <p className="text-sm text-muted-foreground">
                      {strikesRef.current > 5
                        ? 'This is taking a bit longer than usual, please hang tight.'
                        : 'Please wait...'}
                    </p>
                  </div>
                </>
              ) : backendStatus === 'error' ? (
                <>
                  <div className="flex h-16 w-16 items-center justify-center rounded-full border border-accent-border bg-surface-accent-soft">
                    <svg
                      className="h-8 w-8 text-primary"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                      />
                    </svg>
                  </div>
                  <div>
                    <p className="font-medium">Backend Connection Issue</p>
                    <p className="text-sm text-muted-foreground mb-4">
                      The server is taking longer than expected to respond.
                    </p>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        setBackendStatus('connecting')
                        strikesRef.current = 0
                      }}
                      className="border-accent-border/70 hover:bg-surface-accent-soft"
                    >
                      Retry Connection
                    </Button>
                  </div>
                </>
              ) : (
                <>
                  <div className="flex h-16 w-16 items-center justify-center rounded-full bg-surface-elevated">
                    <svg
                      className="h-8 w-8 text-muted-foreground"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
                      />
                    </svg>
                  </div>
                  <div>
                    <p className="font-medium">No Preview</p>
                    <p className="text-sm text-muted-foreground">
                      Select a template to start
                    </p>
                  </div>
                </>
              )}
            </div>
          )}
        </div>

        {/* Control Panel - Right */}
        <div className="w-96 overflow-y-auto border-l border-border/70 bg-card/60 backdrop-blur-sm">
          <ControlPanel
            config={config}
            onConfigChange={setConfig}
            onApply={(updatedConfig) => handleGeneratePreview(updatedConfig)}
          />
        </div>
      </div>
    </div>
  )
}

export default App
