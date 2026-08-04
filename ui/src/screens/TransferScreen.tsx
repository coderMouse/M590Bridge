import { X, Pause } from 'lucide-react'
import { mockTransfer } from '@/lib/mock'
import { PrimaryButton } from '@/components/PrimaryButton'
import { cn } from '@/lib/cn'

export function TransferScreen({ dark }: { dark?: boolean }) {
  return (
    <div className={cn('flex h-full flex-col', dark ? 'bg-[#0F1117] text-[#E8EDF5]' : 'bg-[#F5F7FA] text-[#1A2030]')}>
      <header className={cn('flex items-center justify-between border-b px-4 py-3', dark ? 'border-white/8' : 'border-black/6')}>
        <div>
          <div className="text-[14px] font-bold">文件传输</div>
          <div className={cn('text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>{mockTransfer.title}</div>
        </div>
        <button type="button" className={cn('rounded-md p-1', dark ? 'hover:bg-white/5' : 'hover:bg-black/5')}>
          <X size={16} />
        </button>
      </header>

      <div className="flex-1 space-y-4 px-4 py-4">
        <div
          className={cn(
            'rounded-[10px] border p-3 shadow-[0_8px_28px_rgba(15,23,42,0.08)]',
            dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white',
          )}
        >
          <div className="mb-3 flex items-end justify-between">
            <div>
              <div className="text-[22px] font-bold tabular-nums">{mockTransfer.totalProgress}%</div>
              <div className={cn('text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
                {mockTransfer.speed} · 剩余 {mockTransfer.eta}
              </div>
            </div>
          </div>
          <div className={cn('mb-4 h-2 overflow-hidden rounded-full', dark ? 'bg-[#252D3D]' : 'bg-[#EEF2F8]')}>
            <div className="h-full rounded-full bg-primary" style={{ width: `${mockTransfer.totalProgress}%` }} />
          </div>

          <div className="space-y-3">
            {mockTransfer.files.map((f) => (
              <div key={f.name}>
                <div className="mb-1 flex items-center justify-between gap-2 text-[12px]">
                  <span className="truncate font-medium">{f.name}</span>
                  <span className={cn('shrink-0', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
                    {f.size} · {f.progress}%
                  </span>
                </div>
                <div className={cn('h-1.5 overflow-hidden rounded-full', dark ? 'bg-[#252D3D]' : 'bg-[#EEF2F8]')}>
                  <div className="h-full rounded-full bg-primary/80" style={{ width: `${f.progress}%` }} />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className={cn('flex gap-2 border-t px-4 py-3', dark ? 'border-white/8' : 'border-black/6')}>
        <PrimaryButton variant="secondary" className="flex-1">
          <Pause size={14} /> 暂停
        </PrimaryButton>
        <PrimaryButton variant="danger" className="flex-1">
          取消
        </PrimaryButton>
      </div>
    </div>
  )
}
