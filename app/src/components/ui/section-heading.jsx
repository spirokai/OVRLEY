import { Separator } from '@/components/ui/separator'
import { cn } from '@/lib/utils'

const VARIANT_STYLES = {
  compact: {
    content: 'gap-2 py-3',
    heading: 'text-[10px] tracking-wider',
    icon: 'h-4 w-4',
    root: 'gap-3',
    tag: 'h5',
  },
  drawer: {
    content: 'gap-2',
    heading: 'text-[10px] tracking-wider',
    icon: 'h-4 w-4',
    root: 'gap-3',
    tag: 'h3',
  },
}

/**
 * Renders a shared icon, title, separator, and optional trailing action heading.
 *
 * @param {object} props - Component props.
 * @param {*} props.icon - Heading icon component.
 * @param {string} props.title - Heading text.
 * @param {React.ReactNode} [props.trailing=null] - Optional trailing action.
 * @param {'compact'|'drawer'} [props.variant='compact'] - Canonical heading density.
 * @returns {JSX.Element} Rendered section heading.
 */
export function SectionHeading({ icon: Icon, title, trailing = null, variant = 'compact' }) {
  const styles = VARIANT_STYLES[variant]
  const Heading = styles.tag

  return (
    <div className={cn('flex w-full items-center', styles.root)}>
      <div className={cn('flex min-w-0 flex-1 items-center', styles.content)}>
        <Icon className={cn('text-primary', styles.icon)} />
        <Heading className={cn('font-bold uppercase text-muted-foreground', styles.heading)}>{title}</Heading>
        <Separator className="flex-1" />
      </div>
      {trailing ? <div className="shrink-0">{trailing}</div> : null}
    </div>
  )
}
