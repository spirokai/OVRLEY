import { useEffect, useMemo, useState } from 'react'
import useStore from './store/useStore'
import './index.css'
import * as backend from './api/backend'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import ControlPanel from '@/components/ControlPanel'
import OverlayEditor from '@/components/OverlayEditor'
import OverlayPlayer from '@/components/OverlayPlayer'
import ErrorAlert from '@/components/ErrorAlert'
import RenderProgressOverlay from '@/components/RenderProgressOverlay'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import useBackendStatus, { hasTauriRuntime } from '@/hooks/useBackendStatus'

// Icons
import {
  Play,
  Activity,
  FolderOpen,
  Sparkles,
  Save,
  LayoutGrid,
  Square,
  Minus,
  ZoomIn,
  RotateCcw,
} from 'lucide-react'

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

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
  const config = useStore((state) => state.config)
  const globalDefaults = useStore((state) => state.globalDefaults)
  const setConfig = useStore((state) => state.setConfig)
  const generatingImage = useStore((state) => state.generatingImage)
  const renderingVideo = useStore((state) => state.renderingVideo)
  const setGeneratingImage = useStore((state) => state.setGeneratingImage)
  const setRenderProgress = useStore((state) => state.setRenderProgress)
  const gpxFilename = useStore((state) => state.gpxFilename)
  const setErrorMessage = useStore((state) => state.setErrorMessage)
  const templates = useStore((state) => state.templates)
  const fetchTemplates = useStore((state) => state.fetchTemplates)
  const updateRate = useStore((state) => state.updateRate)
  const exportRange = useStore((state) => state.exportRange)
  const aspectRatio = useStore((state) => state.aspectRatio)
  const loadedTemplateFilename = useStore(
    (state) => state.loadedTemplateFilename,
  )
  const loadedTemplateSource = useStore((state) => state.loadedTemplateSource)
  const setLoadedTemplate = useStore((state) => state.setLoadedTemplate)
  const hydrateTemplateState = useStore((state) => state.hydrateTemplateState)
  const lastSavedTemplateState = useStore(
    (state) => state.lastSavedTemplateState,
  )
  const setLastSavedTemplateState = useStore(
    (state) => state.setLastSavedTemplateState,
  )

  const { backendStatus, backendReady } = useBackendStatus()
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState(
    () => localStorage.getItem('overlayBackgroundMode') || 'checker',
  )

  useEffect(() => {
    localStorage.setItem('overlayBackgroundMode', editorBackgroundMode)
  }, [editorBackgroundMode])

  // Fetch templates once
  useEffect(() => {
    fetchTemplates()
  }, [fetchTemplates])

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

  const currentTemplateState = useMemo(
    () =>
      createTemplateState({
        config,
        globalDefaults,
        updateRate,
        exportRange,
        aspectRatio,
      }),
    [config, globalDefaults, updateRate, exportRange, aspectRatio],
  )
  const status = useMemo(() => {
    if (!config) {
      return null
    }

    if (!lastSavedTemplateState) {
      return 'Draft'
    }

    return templateStatesEqual(currentTemplateState, lastSavedTemplateState)
      ? 'Saved'
      : 'Modified'
  }, [config, currentTemplateState, lastSavedTemplateState])
  const showTemplateStatus = status === 'Draft' || status === 'Modified'
  const sceneWidth = config?.scene?.width || 1920
  const sceneHeight = config?.scene?.height || 1080

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

      await saveFileFromPath(selected)
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

          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1 rounded-2xl border border-border/70 bg-card/80 p-1 backdrop-blur-sm shadow-lg">
              <SimpleTooltip side="bottom" content="Checkered background">
                <Button
                  type="button"
                  variant={
                    editorBackgroundMode === 'checker' ? 'default' : 'ghost'
                  }
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => setEditorBackgroundMode('checker')}
                >
                  <LayoutGrid className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
              <SimpleTooltip side="bottom" content="Black background">
                <Button
                  type="button"
                  variant={
                    editorBackgroundMode === 'black' ? 'default' : 'ghost'
                  }
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => setEditorBackgroundMode('black')}
                >
                  <Square className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
              <div className="mx-1 h-5 w-px bg-border/70" />
              <SimpleTooltip side="bottom" content="Zoom out">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() =>
                    setEditorZoomLevel((current) =>
                      clamp(Number((current - 0.1).toFixed(2)), 0.35, 4),
                    )
                  }
                >
                  <Minus className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
              <div className="min-w-14 text-center text-xs font-semibold text-muted-foreground">
                {Math.round(editorZoomLevel * 100)}%
              </div>
              <SimpleTooltip side="bottom" content="Zoom in">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() =>
                    setEditorZoomLevel((current) =>
                      clamp(Number((current + 0.1).toFixed(2)), 0.35, 4),
                    )
                  }
                >
                  <ZoomIn className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
              <SimpleTooltip side="bottom" content="Reset zoom">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => setEditorZoomLevel(1)}
                >
                  <RotateCcw className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
            </div>
            <div className="rounded-full border border-border/70 bg-card/80 px-3 py-1 text-xs font-medium text-muted-foreground backdrop-blur-sm shadow-lg">
              {sceneWidth} × {sceneHeight}
            </div>
          </div>

          {/* Right - Actions & Status */}
          <div className="flex items-center gap-3">
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
        <div className="relative flex min-w-0 flex-1 flex-col bg-background">
          {generatingImage && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/80">
              <div className="flex flex-col items-center gap-2">
                <Spinner className="h-8 w-8" />
                <span className="text-sm text-muted-foreground">
                  Loading editor data...
                </span>
              </div>
            </div>
          )}
          <div className="min-h-0 flex-1">
            <OverlayEditor
              config={config}
              globalDefaults={globalDefaults}
              onConfigChange={setConfig}
              zoomLevel={editorZoomLevel}
              onZoomLevelChange={setEditorZoomLevel}
              backgroundMode={editorBackgroundMode}
            />
          </div>
          <OverlayPlayer />
        </div>

        {/* Control Panel - Right */}
        <div className="w-96 min-w-96 max-w-96 shrink-0 overflow-y-auto border-l border-border/70 bg-card/60 backdrop-blur-sm">
          <ControlPanel config={config} onConfigChange={setConfig} />
        </div>
      </div>
    </div>
  )
}

export default App
