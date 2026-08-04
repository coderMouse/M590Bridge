import { StatusPill } from '@/components/StatusPill'
import { PrimaryButton } from '@/components/PrimaryButton'
import { Toggle } from '@/components/Toggle'
import { DeviceCard } from '@/components/DeviceCard'
import { ClipboardPreview } from '@/components/ClipboardPreview'
import { HistoryRow } from '@/components/HistoryRow'
import { AppIcon } from '@/components/AppIcon'
import { mockClipboard, mockDevices, mockHistory } from '@/lib/mock'
import { C } from '@/lib/tokens'
import { useState } from 'react'

const swatches = [
  ['primary', C.blue],
  ['hover', C.blueHover],
  ['connected', C.green],
  ['syncing', C.amber],
  ['error', C.red],
  ['bg', C.bg],
  ['card', C.card],
  ['text', C.text],
]

export function UiKitPanel() {
  const [on, setOn] = useState(true)
  return (
    <div className="h-full overflow-auto bg-white p-6 text-[#1A2030]">
      <div className="mb-6 flex items-center gap-3">
        <AppIcon size={36} />
        <div>
          <div className="text-base font-bold">M590Bridge UI Kit</div>
          <div className="text-xs text-[#6B7589]">From Figma Make · tokens + components</div>
        </div>
      </div>

      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-[#6B7589]">Colors</h3>
      <div className="mb-6 flex flex-wrap gap-2">
        {swatches.map(([name, color]) => (
          <div key={name} className="w-16">
            <div className="mb-1 h-10 rounded-md border border-black/8" style={{ background: color }} />
            <div className="truncate text-[10px] text-[#6B7589]">{name}</div>
          </div>
        ))}
      </div>

      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-[#6B7589]">Status</h3>
      <div className="mb-6 flex flex-wrap gap-2">
        {(['已连接', '同步中', '已暂停', '连接中', '未连接', '出错'] as const).map((s) => (
          <StatusPill key={s} status={s} />
        ))}
      </div>

      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-[#6B7589]">Buttons / Toggle</h3>
      <div className="mb-6 grid max-w-sm gap-2">
        <PrimaryButton>主按钮</PrimaryButton>
        <PrimaryButton variant="secondary">次按钮</PrimaryButton>
        <PrimaryButton variant="ghost">文字按钮</PrimaryButton>
        <Toggle label="自动同步" checked={on} onChange={setOn} />
      </div>

      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-[#6B7589]">Cards</h3>
      <div className="mb-4 flex max-w-md gap-2">
        <DeviceCard {...mockDevices.local} />
        <DeviceCard {...mockDevices.peer} />
      </div>
      <div className="mb-4 max-w-md">
        <ClipboardPreview {...mockClipboard} />
      </div>
      <div className="max-w-md rounded-[10px] border border-black/8 bg-white py-1">
        {mockHistory.slice(0, 3).map((h) => (
          <HistoryRow key={h.id} {...h} />
        ))}
      </div>
    </div>
  )
}
