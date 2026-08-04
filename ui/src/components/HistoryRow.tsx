import { FileText, Image, Folder } from 'lucide-react'
import { cn } from '@/lib/cn'

const iconMap = {
  文本: FileText,
  图片: Image,
  文件: Folder,
}

export function HistoryRow({
  type,
  direction,
  time,
  detail,
  dark,
}: {
  type: '文本' | '图片' | '文件'
  direction: string
  time: string
  detail: string
  dark?: boolean
}) {
  const Icon = iconMap[type]
  return (
    <div
      className={cn(
        'flex items-center gap-2.5 rounded-[10px] px-2.5 py-2',
        dark ? 'hover:bg-white/5' : 'hover:bg-[#F1F4F7]',
      )}
    >
      <div
        className={cn(
          'flex size-8 shrink-0 items-center justify-center rounded-lg',
          dark ? 'bg-[#252D3D] text-[#93C5FD]' : 'bg-[#EEF2F8] text-primary',
        )}
      >
        <Icon size={14} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <span className={cn('truncate text-[12px] font-medium', dark ? 'text-[#E8EDF5]' : 'text-[#1A2030]')}>
            {detail}
          </span>
          <span className={cn('shrink-0 text-[11px]', dark ? 'text-[#5A6680]' : 'text-[#9AA3B2]')}>{time}</span>
        </div>
        <div className={cn('mt-0.5 text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
          {type} · {direction}
        </div>
      </div>
    </div>
  )
}
