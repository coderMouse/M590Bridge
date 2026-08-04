import type { ReactNode } from 'react'
import { ChevronRight, ArrowLeft } from 'lucide-react'
import { Toggle } from '@/components/Toggle'
import { PrimaryButton } from '@/components/PrimaryButton'
import { mockDevices } from '@/lib/mock'
import { cn } from '@/lib/cn'
import { useState } from 'react'

function Section({
  title,
  children,
  dark,
}: {
  title: string
  children: ReactNode
  dark?: boolean
}) {
  return (
    <section className="mb-4">
      <h3 className={cn('mb-2 px-1 text-[11px] font-semibold tracking-wide uppercase', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
        {title}
      </h3>
      <div className={cn('overflow-hidden rounded-[10px] border', dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white')}>
        {children}
      </div>
    </section>
  )
}

function Row({
  label,
  value,
  danger,
  dark,
  last,
}: {
  label: string
  value?: string
  danger?: boolean
  dark?: boolean
  last?: boolean
}) {
  return (
    <button
      type="button"
      className={cn(
        'flex w-full items-center justify-between gap-3 px-3 py-3 text-left text-[13px]',
        !last && (dark ? 'border-b border-white/6' : 'border-b border-black/5'),
        danger ? 'text-destructive' : dark ? 'text-[#E8EDF5]' : 'text-[#1A2030]',
      )}
    >
      <span>{label}</span>
      <span className="inline-flex items-center gap-1">
        {value ? <span className={cn('text-[12px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>{value}</span> : null}
        <ChevronRight size={14} className={dark ? 'text-[#5A6680]' : 'text-[#9AA3B2]'} />
      </span>
    </button>
  )
}

export function SettingsScreen({ dark }: { dark?: boolean }) {
  const [text, setText] = useState(true)
  const [image, setImage] = useState(true)
  const [filePaste, setFilePaste] = useState(true)
  const [n1, setN1] = useState(true)
  const [n2, setN2] = useState(true)
  const [n3, setN3] = useState(true)

  return (
    <div className={cn('flex h-full flex-col', dark ? 'bg-[#0F1117] text-[#E8EDF5]' : 'bg-[#F5F7FA] text-[#1A2030]')}>
      <header className={cn('flex items-center gap-2 border-b px-3 py-3', dark ? 'border-white/8' : 'border-black/6')}>
        <button type="button" className={cn('rounded-md p-1', dark ? 'hover:bg-white/5' : 'hover:bg-black/5')}>
          <ArrowLeft size={16} />
        </button>
        <div className="text-[14px] font-bold">设置</div>
      </header>

      <div className="flex-1 overflow-auto px-4 py-4">
        <Section title="设备" dark={dark}>
          <Row label="本机显示名" value={mockDevices.local.name} dark={dark} />
          <Row label="当前对端" value={mockDevices.peer.name} dark={dark} />
          <Row label="重新配对" dark={dark} />
          <Row label="解除配对" danger dark={dark} last />
        </Section>

        <Section title="同步" dark={dark}>
          <div className={cn('space-y-3 px-3 py-3', dark ? 'border-b border-white/6' : 'border-b border-black/5')}>
            <Toggle label="自动同步文本" checked={text} onChange={setText} dark={dark} />
            <Toggle label="自动同步图片" checked={image} onChange={setImage} dark={dark} />
            <Toggle label="文件仅在粘贴时传输" checked={filePaste} onChange={setFilePaste} dark={dark} />
          </div>
          <Row label="保留最近历史" value="20 条" dark={dark} last />
        </Section>

        <Section title="网络" dark={dark}>
          <Row label="发现方式" value="自动 (mDNS)" dark={dark} />
          <Row label="端口（高级）" value="默认" dark={dark} />
          <Row label="仅允许已配对设备" value="开" dark={dark} last />
        </Section>

        <Section title="通知" dark={dark}>
          <div className="space-y-3 px-3 py-3">
            <Toggle label="同步成功通知" checked={n1} onChange={setN1} dark={dark} />
            <Toggle label="传输完成通知" checked={n2} onChange={setN2} dark={dark} />
            <Toggle label="断开连接通知" checked={n3} onChange={setN3} dark={dark} />
          </div>
        </Section>

        <Section title="关于" dark={dark}>
          <Row label="版本" value="0.1.0" dark={dark} />
          <Row label="开源许可" dark={dark} last />
        </Section>
      </div>
    </div>
  )
}

export function UnpairModal({ dark }: { dark?: boolean }) {
  return (
    <div className={cn('flex h-full items-center justify-center p-6', dark ? 'bg-[#0A0D12]/80' : 'bg-[#0F172A]/35')}>
      <div
        className={cn(
          'w-full max-w-[320px] rounded-[14px] p-5 shadow-[0_8px_28px_rgba(15,23,42,0.16)]',
          dark ? 'bg-[#1C2030] text-[#E8EDF5]' : 'bg-white text-[#1A2030]',
        )}
      >
        <h3 className="m-0 mb-2 text-[16px] font-bold">解除配对？</h3>
        <p className={cn('m-0 mb-5 text-[13px] leading-5', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
          解除后需重新输入配对码才能同步。
        </p>
        <div className="flex gap-2">
          <PrimaryButton variant="secondary" className="flex-1">
            取消
          </PrimaryButton>
          <PrimaryButton variant="danger" className="flex-1">
            解除配对
          </PrimaryButton>
        </div>
      </div>
    </div>
  )
}
