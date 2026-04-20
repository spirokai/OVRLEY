import { Type } from 'lucide-react'
import { cn } from '@/lib/utils'
import { WIDGET_ICONS } from './constants'
import WidgetPreview from './WidgetPreview'
import { buildWidgetTransform } from './utils'

export default function OverlayCanvas({
  widgets,
  draftWidgets,
  globalScale,
  globalOpacity,
  activity,
  sampleIndex,
  backgroundMode,
  sceneSize,
  setSceneElement,
  widgetRefCallbacks,
  setSelectedWidgetId,
}) {
  return (
    <div
      ref={setSceneElement}
      className="relative overflow-visible"
      style={{
        width: sceneSize.width,
        height: sceneSize.height,
      }}
    >
      <div
        className={cn(
          'absolute inset-0 overflow-hidden rounded-md shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]',
          backgroundMode === 'checker' && 'bg-overlay-grid-muted',
        )}
        style={{ backgroundColor: '#000000' }}
      >
        <div className="absolute inset-0 " />
        {widgets.map((widget) => {
          const draft = draftWidgets[widget.id]
          const previewWidget = draft
            ? {
                ...widget,
                data: {
                  ...widget.data,
                  ...draft,
                },
              }
            : widget
          const x = previewWidget.data.x ?? 0
          const y = previewWidget.data.y ?? 0
          const scale = (draft?.scale ?? 1) * globalScale
          const rotation =
            previewWidget.type === 'course'
              ? (previewWidget.data.rotation ?? 0)
              : 0
          const width = previewWidget.data.width
          const height = previewWidget.data.height
          const Icon = WIDGET_ICONS[previewWidget.type] || Type

          return (
            <div
              key={previewWidget.id}
              ref={widgetRefCallbacks[previewWidget.id]}
              className="group absolute cursor-move select-none rounded-xl border border-transparent transition-shadow"
              style={{
                left: x,
                top: y,
                width,
                height,
                transform: buildWidgetTransform({ scale, rotation }),
                transformOrigin:
                  previewWidget.type === 'course'
                    ? 'center center'
                    : 'top left',
              }}
              onMouseDown={(event) => {
                event.stopPropagation()
                setSelectedWidgetId(previewWidget.id)
              }}
            >
              <div className="absolute -top-7 left-0 flex items-center gap-1 rounded-full border border-border/70 bg-card/80 px-2 py-1 text-[10px] font-semibold text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100">
                <Icon className="h-3 w-3" />
                <span>{previewWidget.type}</span>
              </div>
              <WidgetPreview
                widget={previewWidget}
                activity={activity}
                sampleIndex={sampleIndex}
                globalOpacity={globalOpacity}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}
