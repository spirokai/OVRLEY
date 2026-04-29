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

export default function LoadingOverlay({
  label = 'Loading editor data...',
  show,
}) {
  if (!show) {
    return null
  }

  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/80">
      <div className="flex flex-col items-center gap-2">
        <Spinner className="h-8 w-8" />
        <span className="text-sm text-muted-foreground">{label}</span>
      </div>
    </div>
  )
}
