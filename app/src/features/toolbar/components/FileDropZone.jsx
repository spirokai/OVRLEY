import { createPortal } from 'react-dom'
import { FileUp } from 'lucide-react'
import { cn } from '@/lib/utils'

/**
 * Renders a floating drag cursor portal while a file is being dragged.
 *
 * @param {object} props - Component props.
 * @param {{x: number, y: number}|null} props.position - Cursor position in viewport coordinates.
 * @returns {JSX.Element|null} Rendered cursor portal.
 */
export function FileDragCursor({ position }) {
  if (!position) return null

  return createPortal(
    <div
      aria-hidden="true"
      className="cursor-pointer fixed z-1000 flex h-14 w-14 -translate-x-2 -translate-y-2 items-center justify-center rounded-md border-2 border-card bg-foreground text-card shadow-2xl"
      style={{ left: position.x, top: position.y }}
    >
      <FileUp className="h-10 w-10" strokeWidth={1.5} />
    </div>,
    document.body,
  )
}

/**
 * Renders a reusable dashed file drop zone.
 *
 * @param {object} props - Component props.
 * @param {React.RefObject} props.dropZoneRef - Ref attached to the drop zone element.
 * @param {boolean} props.isOverDropZone - Whether a drag is currently over the zone.
 * @param {object} props.dropZoneProps - Event handlers to spread onto the zone.
 * @param {string} props.label - Primary drop zone label.
 * @param {string} props.sublabel - Supported file types label.
 * @param {React.ElementType} [props.icon=FileUp] - Icon component to display.
 * @returns {JSX.Element} Rendered drop zone.
 */
export function FileDropZone({ dropZoneRef, isOverDropZone, dropZoneProps, label, sublabel, icon: Icon = FileUp }) {
  return (
    <div
      ref={dropZoneRef}
      className={cn(
        'relative mt-4 flex min-h-24 shrink-0 flex-col items-center justify-center rounded-xs border border-dashed border-border/80 bg-surface px-4 text-center transition-colors',
        isOverDropZone && 'border-primary bg-surface-accent-soft/30',
      )}
      {...dropZoneProps}
    >
      <Icon className={cn('mb-2 h-5 w-5 text-muted-foreground', isOverDropZone && 'text-primary')} />
      <p className="text-xs font-extrabold text-foreground">{label}</p>
      <p className="mt-1 text-[0.75rem] leading-tight text-muted-foreground">{sublabel}</p>
    </div>
  )
}
