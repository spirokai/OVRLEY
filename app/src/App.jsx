import { useEffect, useState, useRef, useCallback } from 'react'
import useStore from './store/useStore'
import './index.css'
import * as backend from './api/backend'
import { applyGlobalDefaults } from './lib/config-utils'

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
    input.accept = '.gpx'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

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
    loadedTemplateFilename,
    setLoadedTemplateFilename,
    lastSavedConfig,
    setLastSavedConfig,
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
      const data = await backend.loadTemplate(filename)
      setLoadedTemplateFilename(filename)
      setConfig(data)
      setLastSavedConfig(data)

      // Auto-refresh preview with new template
      handleGeneratePreview(data)
    } catch (err) {
      console.error('Failed to load template:', err)
      setErrorMessage(`Failed to load template: ${err.message}`)
    } finally {
      setGeneratingImage(false)
    }
  }

  const handleSaveTemplate = async () => {
    let filename = loadedTemplateFilename

    // If draft or built-in, prompt for new name
    const currentTemplate = templates.find((t) => t.id === filename)
    if (!filename || currentTemplate?.type === 'built-in') {
      const name = prompt(
        'Enter a name for your new template:',
        filename?.replace('.json', '') || 'my_template',
      )
      if (!name) return
      filename = name.toLowerCase().replace(/\s+/g, '_')
      if (!filename.endsWith('.json')) filename += '.json'
    }

    try {
      await backend.saveTemplate(filename, config)
      setLoadedTemplateFilename(filename)
      setLastSavedConfig(config)
      fetchTemplates() // Refresh list
    } catch (err) {
      console.error('Failed to save template:', err)
      setErrorMessage(`Failed to save template: ${err.message}`)
    }
  }

  const handleOpenFolder = async () => {
    try {
      await backend.openTemplatesFolder()
    } catch (err) {
      console.error('Failed to open folder:', err)
    }
  }

  const getStatus = () => {
    if (!loadedTemplateFilename) return 'Draft'
    if (!lastSavedConfig) return 'Saved'
    const isModified =
      JSON.stringify(config) !== JSON.stringify(lastSavedConfig)
    return isModified ? 'Modified' : 'Saved'
  }

  const status = getStatus()

  // Generate preview
  const handleGeneratePreview = useCallback(
    async (configOverride = null) => {
      const baseConfig = configOverride || config
      if (!baseConfig) return

      // Apply global defaults before sending to backend
      const currentConfig = applyGlobalDefaults(baseConfig, globalDefaults)

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
      const selected = hasTauriRuntime()
        ? await (async () => {
            const { open } = await import('@tauri-apps/plugin-dialog')
            return open({
              multiple: false,
              filters: [{ name: 'GPX', extensions: ['gpx'] }],
              title: 'Select GPX Activity',
            })
          })()
        : await selectBrowserGpxFile()

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
    <div className="h-screen dark flex flex-col bg-[#0a0a0a] text-foreground">
      <ErrorAlert />
      <RenderProgressOverlay />
      {/* Header */}
      <header className="relative z-50 border-b border-border bg-card/50 backdrop-blur-sm shrink-0">
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

            <div className="h-8 w-px bg-zinc-800/50" />

            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Select
                  value={loadedTemplateFilename || ''}
                  onValueChange={handleTemplateChange}
                >
                  <SelectTrigger className="w-56 h-8 text-xs border-zinc-700/50 bg-zinc-900/30">
                    <div className="flex items-center gap-2 truncate">
                      <Sparkles className="h-3 w-3 text-red-500 shrink-0" />
                      <SelectValue placeholder="Select Template..." />
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

                <Badge
                  variant={
                    status === 'Modified'
                      ? 'secondary'
                      : status === 'Draft'
                        ? 'outline'
                        : 'default'
                  }
                  className={`text-[10px] h-5 ${
                    status === 'Modified'
                      ? 'bg-orange-500/10 text-orange-500 border-orange-500/20'
                      : ''
                  }`}
                >
                  {status}
                </Badge>
              </div>

              <div className="flex items-center gap-1">
                {(status === 'Modified' || status === 'Draft') && config && (
                  <SimpleTooltip side="bottom" content="Save Template">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 text-muted-foreground hover:text-red-500 hover:bg-red-500/10"
                      onClick={handleSaveTemplate}
                    >
                      <Save className="h-4 w-4" />
                    </Button>
                  </SimpleTooltip>
                )}
                <SimpleTooltip side="bottom" content="Open Templates Folder">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground hover:text-foreground hover:bg-zinc-800"
                    onClick={handleOpenFolder}
                  >
                    <FolderOpen className="h-4 w-4" />
                  </Button>
                </SimpleTooltip>
              </div>
            </div>
          </div>

          {/* Right - Actions & Status */}
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                className="gap-2 h-8 px-3 border-zinc-700/50 hover:bg-zinc-800/50 text-muted-foreground hover:text-foreground"
                onClick={handleGpxFileOpen}
              >
                <Activity className="h-3.5 w-3.5" />
                <span className="max-w-25 truncate">
                  {gpxFilename === 'demo.gpxinit'
                    ? 'Load GPX'
                    : gpxFilename || 'Load GPX'}
                </span>
              </Button>
            </div>

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
                    ? 'border-red-500 bg-red-500/10 text-foreground ring-1 ring-red-500/50'
                    : 'border-red-500/30 hover:border-red-500/50 hover:bg-red-500/5'
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
                    className={`h-3.5 w-3.5 ${hasUnrenderedChanges ? 'text-red-400' : 'text-red-500'}`}
                  />
                )}
                <span>Refresh Preview</span>
                {hasUnrenderedChanges && !generatingImage && (
                  <span className="absolute -top-1 -right-1 flex h-2.5 w-2.5">
                    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                    <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-red-500"></span>
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
                className="bg-red-600 hover:bg-red-700 text-white"
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
                className="gap-2 h-8 px-3 border-red-500/30 hover:border-red-500/50 hover:bg-red-500/5 text-muted-foreground hover:text-foreground"
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
        <div className="flex-1 flex items-center justify-center p-8 bg-[#0a0a0a]">
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
                className="max-w-full max-h-full object-contain rounded-lg shadow-2xl border border-zinc-800 bg-grid-transparent"
                onError={() => setImageError(true)}
              />
              {config?.scene && (
                <div className="absolute top-4 left-1/2 -translate-x-1/2">
                  <div className="px-3 py-1 rounded-full bg-zinc-950/80 backdrop-blur-sm border border-zinc-700">
                    <span className="text-xs font-medium text-zinc-400">
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
                  <div className="w-16 h-16 rounded-full bg-secondary flex items-center justify-center">
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
                  <div className="w-16 h-16 rounded-full bg-destructive/10 flex items-center justify-center border border-destructive/20">
                    <svg
                      className="w-8 h-8 text-destructive"
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
                      className="border-red-500/30 hover:bg-red-500/10"
                    >
                      Retry Connection
                    </Button>
                  </div>
                </>
              ) : (
                <>
                  <div className="w-16 h-16 rounded-full bg-secondary flex items-center justify-center">
                    <svg
                      className="w-8 h-8 text-muted-foreground"
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
        <div className="w-96 border-l border-border bg-card/30 backdrop-blur-sm overflow-y-auto">
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
