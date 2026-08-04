import { Monitor } from 'lucide-react'
import { cn } from '@/lib/cn'

export function DeviceCard({
  name,
  os,
  kind,
  dark,
}: {
  name: string
  os: string
  kind: '本机' | '对端'
  dark?: boolean
}) {
  return (
    <div
      className={cn(
        'flex min-w-0 flex-1 items-center gap-2.5 rounded-[10px] border px-3 py-2.5',
        dark ? 'border-white/10 bg-[#252D3D]' : 'border-black/8 bg-[#F1F4F7]',
      )}
    >
      <div
        className={cn(
          'flex size-8 items-center justify-center rounded-lg',
          dark ? 'bg-[#1C2030] text-primary' : 'bg-white text-primary shadow-sm',
        )}
      >
        <Monitor size={16} />
      </div>
      <div className="min-w-0">
        <div className={cn('truncate text-[12px] font-semibold', dark ? 'text-[#E8EDF5]' : 'text-[#1A2030]')}>
          {name}
        </div>
        <div className={cn('truncate text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
          {kind} · {os}
        </div>
      </div>
    </div>
  )
}
