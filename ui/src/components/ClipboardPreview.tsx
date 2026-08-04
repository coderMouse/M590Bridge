import { FileText, Image, Folder } from 'lucide-react'
import { cn } from '@/lib/cn'

const iconMap = {
  文本: FileText,
  图片: Image,
  文件: Folder,
}

export function ClipboardPreview({
  type,
  preview,
  meta,
  dark,
}: {
  type: '文本' | '图片' | '文件'
  preview: string
  meta: string
  dark?: boolean
}) {
  const Icon = iconMap[type]
  return (
    <div
      className={cn(
        'rounded-[10px] border p-3 shadow-[0_4px_16px_rgba(15,23,42,0.06)]',
        dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white',
      )}
    >
      <div className="mb-2 flex items-center gap-2">
        <span
          className={cn(
            'inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium',
            dark ? 'bg-[#252D3D] text-[#93C5FD]' : 'bg-[#EFF6FF] text-primary',
          )}
        >
          <Icon size={12} />
          {type}
        </span>
        <span className={cn('text-[11px]', dark ? 'text-[#5A6680]' : 'text-[#9AA3B2]')}>当前剪贴板</span>
      </div>
      <p className={cn('m-0 line-clamp-2 text-[13px] leading-5', dark ? 'text-[#E8EDF5]' : 'text-[#1A2030]')}>
        {preview}
      </p>
      <p className={cn('m-0 mt-2 text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>{meta}</p>
    </div>
  )
}
