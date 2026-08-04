import { cn } from '@/lib/cn'

export function TrayMenu({
  platform = 'windows',
  dark,
}: {
  platform?: 'windows' | 'linux'
  dark?: boolean
}) {
  const items = [
    { label: '已连接到 书房电脑', muted: true },
    { label: '已同步文本：会议纪要…', muted: true },
    { type: 'sep' as const },
    { label: '暂停同步' },
    { label: '手动发送当前剪贴板' },
    { label: '打开主面板' },
    { label: '设置' },
    { type: 'sep' as const },
    { label: '退出' },
  ]

  return (
    <div
      className={cn(
        'w-[220px] overflow-hidden py-1.5 text-[12px] shadow-[0_8px_28px_rgba(15,23,42,0.14)]',
        platform === 'linux' ? 'rounded-xl' : 'rounded-md',
        dark ? 'border border-white/10 bg-[#1C2030] text-[#E8EDF5]' : 'border border-black/8 bg-white text-[#1A2030]',
      )}
    >
      {items.map((item, idx) =>
        'type' in item && item.type === 'sep' ? (
          <div key={idx} className={cn('my-1 h-px', dark ? 'bg-white/8' : 'bg-black/6')} />
        ) : (
          <button
            key={idx}
            type="button"
            className={cn(
              'flex w-full px-3 py-1.5 text-left',
              item.muted
                ? dark
                  ? 'text-[#8896AC]'
                  : 'text-[#6B7589]'
                : dark
                  ? 'hover:bg-white/5'
                  : 'hover:bg-[#F1F4F7]',
            )}
          >
            {item.label}
          </button>
        ),
      )}
    </div>
  )
}

const notes = [
  { title: '剪贴板已同步', body: '文本已同步到 Windows 10', tone: 'success' as const },
  { title: '收到文件', body: '来自 Ubuntu 的 3 个文件，粘贴后开始接收', tone: 'info' as const },
  { title: '传输完成', body: '128MB · 用时 1.4s', tone: 'success' as const },
  { title: '连接已断开', body: '正在重试…', tone: 'warning' as const },
]

export function NotificationsSet({ dark }: { dark?: boolean }) {
  return (
    <div className={cn('flex h-full flex-col gap-2 p-3', dark ? 'bg-[#0F1117]' : 'bg-[#E8ECF2]')}>
      {notes.map((n) => (
        <div
          key={n.title}
          className={cn(
            'rounded-[10px] border px-3 py-2.5 shadow-sm',
            dark ? 'border-white/10 bg-[#1C2030] text-[#E8EDF5]' : 'border-black/8 bg-white text-[#1A2030]',
          )}
        >
          <div className="text-[12px] font-semibold">{n.title}</div>
          <div className={cn('mt-0.5 text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>{n.body}</div>
        </div>
      ))}
    </div>
  )
}
