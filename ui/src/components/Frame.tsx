import type { ReactNode } from 'react'

export function Frame({
  label,
  sublabel,
  children,
  width,
  height,
  canvasBg = '#D8DCE4',
}: {
  label: string
  sublabel?: string
  children: ReactNode
  width: number
  height?: number
  canvasBg?: string
}) {
  return (
    <div className="flex shrink-0 flex-col items-start">
      <div className="mb-2.5">
        <div className="text-xs font-semibold tracking-wide text-slate-600">{label}</div>
        {sublabel ? <div className="mt-0.5 text-xs text-slate-400">{sublabel}</div> : null}
      </div>
      <div
        className="relative shrink-0 overflow-hidden rounded-xl"
        style={{
          width,
          height,
          background: canvasBg,
          boxShadow: '0 0 0 1px rgba(0,0,0,0.06), 0 4px 16px rgba(0,0,0,0.10)',
        }}
      >
        {children}
      </div>
    </div>
  )
}

export function FrameRow({ children }: { children: ReactNode }) {
  return <div className="mb-10 flex flex-wrap gap-8">{children}</div>
}

export function CanvasSectionHeading({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div className="mb-5">
      <h2 className="m-0 text-lg font-semibold text-slate-800">{title}</h2>
      {subtitle ? <p className="m-0 mt-1 text-sm text-slate-500">{subtitle}</p> : null}
    </div>
  )
}
