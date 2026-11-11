import { AlertTriangle, Blend, Clock3, Film, ImageOff, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { TransitionSelect } from '@/features/transitions'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { cn } from '@/lib/utils'
import { useTransitionDefaultsSection } from '../hooks/useTransitionDefaultsSection'
import { useVideoCards } from '../hooks/useVideoCards'
import { useVideoSyncControls } from '../hooks/useVideoSyncControls'

function LabeledField({ id, label, children }) {
  return (
    <div className="min-w-0 space-y-1">
      <Label htmlFor={id} className="text-xs font-bold text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  )
}

/** Sync of every video at once; the video drawer holds the same settings for one video. */
function SyncAllVideosSection() {
  const { t } = useTranslation()
  const sync = useVideoSyncControls()
  // Rows like the video drawer's sync section: the label on the left, its control on the right.
  const rowLabel = 'flex min-h-8 items-center pl-0 text-xs font-bold text-muted-foreground'
  return (
    <section className="space-y-3" aria-label={t('toolbar.syncAllVideos', 'Sync all videos')}>
      <SectionHeading icon={Clock3} title={t('toolbar.syncAllVideos', 'Sync all videos')} variant="drawer" />
      <div className="grid grid-cols-2 items-start gap-x-4.5 gap-y-3 px-2">
        <Button type="button" variant="secondary" className="col-span-2 w-full gap-2" onClick={sync.syncAllVideos}>
          <RefreshCw className="size-4" /> {t('toolbar.syncAllVideos', 'Sync all videos')}
        </Button>
        <span className={rowLabel}>{t('toolbar.creationTime', 'Creation Time')}</span>
        <Tabs
          value={sync.canResetCreationTime ? 'filename' : 'detected'}
          onValueChange={(value) => (value === 'filename' ? sync.setVideoCreationTimesFromFilenames() : sync.resetVideoCreationTimes())}
        >
          <TabsList variant="toolbar" className="grid h-8 w-full grid-cols-2 p-0.5">
            <TabsTrigger variant="toolbar" value="detected" className="h-full px-2 text-[0.7rem]">
              {t('toolbar.detected', 'Detected')}
            </TabsTrigger>
            <TabsTrigger variant="toolbar" value="filename" className="h-full px-2 text-[0.7rem]" disabled={!sync.filenameCreationTimeAvailable}>
              {t('toolbar.filename', 'Filename')}
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {sync.timezone !== null ? (
          <>
            <Label htmlFor="videos-timezone" className={rowLabel}>
              {t('toolbar.applyTimezone', 'Apply Timezone')}
            </Label>
            <div className="flex min-h-8 items-center">
              <Switch
                id="videos-timezone"
                checked={sync.videoSyncTimezoneMode === 'utc'}
                disabled={sync.timezoneDisabled}
                onCheckedChange={(checked) => sync.setVideoSyncTimezoneMode(checked ? 'utc' : 'local')}
              />
            </div>
          </>
        ) : null}
      </div>
    </section>
  )
}

function TransitionDefaultsSection() {
  const { t } = useTranslation()
  const section = useTransitionDefaultsSection()
  return (
    <section className="space-y-3" aria-label={t('toolbar.transitionDefaults', 'Default transitions')}>
      <SectionHeading
        icon={Blend}
        title={t('toolbar.transitionDefaults', 'Default transitions')}
        variant="drawer"
        help={t(
          'toolbar.transitionDefaultsHint',
          "Used at every join you have not set yourself. Separate videos closer together than the continuity threshold, such as a camera's split recording, are joined without a transition.",
        )}
      />
      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <LabeledField id="transition-defaults-type" label={t('video-preview.transitionType', 'Transition')}>
          <TransitionSelect
            id="transition-defaults-type"
            value={section.type}
            label={section.typeLabel}
            previewType={section.previewType}
            onSelect={section.onTypeChange}
          />
        </LabeledField>
        <LabeledField id="transition-defaults-duration" label={t('video-preview.transitionDuration', 'Duration')}>
          <SecondsInput
            id="transition-defaults-duration"
            value={section.durationSeconds}
            fps={section.durationFps}
            mode="duration"
            display="seconds"
            onCommit={section.onDurationCommit}
            className="h-9 text-xs"
          />
        </LabeledField>
        <LabeledField id="transition-defaults-threshold" label={t('video-preview.continuityThreshold', 'Continuity threshold')}>
          <SecondsInput
            id="transition-defaults-threshold"
            value={section.thresholdSeconds}
            fps={null}
            onCommit={section.onThresholdCommit}
            className="h-9 text-xs"
          />
        </LabeledField>
      </div>
    </section>
  )
}

function VideoCardPicture({ card }) {
  const { t } = useTranslation()
  if (card.missing) {
    return (
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-1.5 bg-destructive/15 px-2 text-center text-destructive">
        <AlertTriangle className="h-5 w-5" aria-hidden="true" />
        <span className="text-xs font-bold">{t('toolbar.videoNotFound', 'Video not found')}</span>
      </div>
    )
  }
  if (card.thumbnailUrl === null) {
    return (
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-1.5 px-2 text-center text-muted-foreground">
        <ImageOff className="h-5 w-5" aria-hidden="true" />
        <span className="text-xs font-bold">{t('toolbar.noVideoFrame', 'No frame could be read')}</span>
      </div>
    )
  }
  return <img src={card.thumbnailUrl} alt="" className="size-full object-contain" />
}

/**
 * One video: its thumbnail (or why there is none), name, and length. Selecting it
 * selects its clip, as on the timeline, and shows its details in the video drawer;
 * the trash button (shown on hover and focus) or the Delete key removes it from the project.
 */
function VideoCard({ card }) {
  const { t } = useTranslation()
  const handleKeyDown = (event) => {
    if (event.defaultPrevented || matchKeyboardShortcut(event, 'video-card')?.commandId !== 'videoCard.remove') return
    event.preventDefault()
    card.onRemove()
  }
  return (
    <li className="group relative">
      <button
        type="button"
        aria-pressed={card.isSelected}
        aria-keyshortcuts="Delete Backspace"
        className={cn(
          'flex w-full cursor-pointer flex-col overflow-hidden rounded-xs border text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-foreground',
          card.isSelected ? 'border-primary bg-surface-accent-soft' : 'border-transparent bg-surface-darken hover:border-border',
        )}
        onClick={card.onSelect}
        onKeyDown={handleKeyDown}
      >
        <span className="relative block aspect-video w-full bg-black">
          <VideoCardPicture card={card} />
          <span className="absolute bottom-1 right-1 rounded-xs bg-black/70 px-1 text-[0.65rem] font-bold tabular-nums text-white">
            {card.durationLabel}
          </span>
        </span>
        <span className="truncate px-2 py-1.5 text-xs font-bold text-foreground" title={card.filename}>
          {card.filename}
        </span>
      </button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="absolute right-1 top-1 h-6 w-6 bg-black/60 text-white opacity-0 hover:bg-surface-accent-soft hover:text-primary group-focus-within:opacity-100 group-hover:opacity-100"
        onClick={card.onRemove}
        aria-label={t('toolbar.removeVideo', { defaultValue: 'Remove {{name}} from project', name: card.filename })}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </li>
  )
}

/**
 * The video tab's companion panel, in place of (and as wide as) the widget sidebar:
 * the sync of every video and the default transitions above a two-column grid of video cards in timeline order.
 *
 * @param {object} props - Component props.
 * @param {() => void} props.onDeleteVideo - Removes every imported video.
 * @param {(videoId: string) => void} props.onRemoveVideo - Removes one imported video.
 * @returns {JSX.Element} Rendered panel.
 */
export function VideoPanel({ onDeleteVideo, onRemoveVideo }) {
  const { t } = useTranslation()
  const cards = useVideoCards({ onRemoveVideo })

  return (
    <aside
      className="relative z-60 flex h-full w-106 min-w-106 max-w-106 shrink-0 flex-col overflow-y-auto border-l border-border bg-card px-4.5 pb-4 thin-scrollbar"
      data-keeps-clip-selection
    >
      {cards.length > 0 ? (
        <div className="space-y-8 py-4">
          <SyncAllVideosSection />
          <TransitionDefaultsSection />
          <section className="space-y-3" aria-label={t('toolbar.importedVideos', 'Imported videos')}>
            <SectionHeading
              icon={Film}
              title={t('toolbar.importedVideos', 'Imported videos')}
              variant="drawer"
              trailing={
                <Button
                  type="button"
                  variant="ghost"
                  className="h-7 gap-1.5 px-2 text-xs text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  onClick={onDeleteVideo}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t('toolbar.removeAll', 'Remove all')}
                </Button>
              }
            />
            <ul className="grid grid-cols-2 gap-3" aria-label={t('toolbar.videos', 'Videos')}>
              {cards.map((card) => (
                <VideoCard key={card.id} card={card} />
              ))}
            </ul>
          </section>
        </div>
      ) : (
        <p className="px-2 pt-6 text-center text-xs text-muted-foreground">{t('toolbar.noVideos', 'No videos imported yet.')}</p>
      )}
    </aside>
  )
}
