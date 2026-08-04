import { Link2, Settings, Pause, WifiOff } from 'lucide-react'
import { StatusPill } from '@/components/StatusPill'
import { DeviceCard } from '@/components/DeviceCard'
import { ClipboardPreview } from '@/components/ClipboardPreview'
import { HistoryRow } from '@/components/HistoryRow'
import { Toggle } from '@/components/Toggle'
import { mockClipboard, mockDevices, mockHistory } from '@/lib/mock'
import type { ConnectionStatus } from '@/lib/tokens'
import { cn } from '@/lib/cn'
import { useState } from 'react'

export function HomeScreen({
  status = '已连接',
  dark,
  disconnected,
}: {
  status?: ConnectionStatus
  dark?: boolean
  disconnected?: boolean
}) {
  const [autoSync, setAutoSync] = useState(true)
  const [fileOnPaste, setFileOnPaste] = useState(true)
  const effective: ConnectionStatus = disconnected ? '未连接' : status

  return (
    <div className={cn('flex h-full flex-col', dark ? 'bg-[#0F1117] text-[#E8EDF5]' : 'bg-[#F5F7FA] text-[#1A2030]')}>
      <header
        className={cn(
          'flex items-center justify-between border-b px-4 py-3',
          dark ? 'border-white/8' : 'border-black/6',
        )}
      >
        <div className="text-[14px] font-bold">M590Bridge</div>
        <StatusPill status={effective} dark={dark} />
      </header>

      {disconnected ? (
        <div className="mx-4 mt-3 flex items-center gap-2 rounded-lg bg-status-error-bg px-3 py-2 text-[12px] text-status-error">
          <WifiOff size={14} />
          <span className="flex-1">连接已断开，正在重试…</span>
          <button type="button" className="font-semibold underline-offset-2 hover:underline">
            手动重连
          </button>
        </div>
      ) : null}

      <div className="flex-1 space-y-4 overflow-auto px-4 py-4">
        <div className="flex items-center gap-2">
          <DeviceCard {...mockDevices.local} dark={dark} />
          <Link2 size={16} className={cn('shrink-0', disconnected ? 'text-[#9AA3B2]' : 'text-primary')} />
          <DeviceCard {...mockDevices.peer} dark={dark} />
        </div>

        <ClipboardPreview {...mockClipboard} dark={dark} />

        <div>
          <div className={cn('mb-1.5 text-[12px] font-semibold', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
            最近同步
          </div>
          <div
            className={cn(
              'rounded-[10px] border py-1',
              dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white',
            )}
          >
            {mockHistory.map((item) => (
              <HistoryRow key={item.id} {...item} dark={dark} />
            ))}
          </div>
        </div>
      </div>

      <footer
        className={cn(
          'space-y-3 border-t px-4 py-3',
          dark ? 'border-white/8 bg-[#161B27]' : 'border-black/6 bg-white',
        )}
      >
        <Toggle label="自动同步剪贴板" checked={autoSync} onChange={setAutoSync} dark={dark} />
        <Toggle label="文件粘贴时再传输" checked={fileOnPaste} onChange={setFileOnPaste} dark={dark} />
        <div className="flex items-center justify-between pt-1">
          <button
            type="button"
            className={cn('inline-flex items-center gap-1 text-[12px] font-medium', dark ? 'text-[#93C5FD]' : 'text-primary')}
          >
            <Settings size={13} /> 设置
          </button>
          <button
            type="button"
            className={cn('inline-flex items-center gap-1 text-[12px] font-medium', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}
          >
            <Pause size={13} /> 暂停同步
          </button>
        </div>
      </footer>
    </div>
  )
}
