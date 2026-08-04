import type { ConnectionStatus } from '@/lib/tokens'
import { cn } from '@/lib/cn'

const map: Record<ConnectionStatus, { dot: string; bg: string; text: string }> = {
  已连接: { dot: 'bg-status-connected', bg: 'bg-status-connected-bg', text: 'text-status-connected' },
  同步中: { dot: 'bg-status-syncing', bg: 'bg-status-syncing-bg', text: 'text-status-syncing' },
  已暂停: { dot: 'bg-status-paused', bg: 'bg-status-paused-bg', text: 'text-status-paused' },
  连接中: { dot: 'bg-primary', bg: 'bg-accent', text: 'text-primary' },
  未连接: { dot: 'bg-status-error', bg: 'bg-status-error-bg', text: 'text-status-error' },
  出错: { dot: 'bg-status-error', bg: 'bg-status-error-bg', text: 'text-status-error' },
}

export function StatusPill({ status, dark }: { status: ConnectionStatus; dark?: boolean }) {
  const s = map[status]
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold',
        s.bg,
        s.text,
        dark && 'ring-1 ring-white/5',
      )}
    >
      <span className={cn('size-1.5 rounded-full', s.dot)} />
      {status}
    </span>
  )
}
