import { Copy, RefreshCw, Monitor, Info } from 'lucide-react'
import { AppIcon } from '@/components/AppIcon'
import { PrimaryButton } from '@/components/PrimaryButton'
import { pairCode, mockDevices } from '@/lib/mock'
import { cn } from '@/lib/cn'

export function PairingScreen({
  state = 'waiting',
  dark,
}: {
  state?: 'waiting' | 'pairing' | 'success' | 'error'
  dark?: boolean
}) {
  return (
    <div className={cn('flex h-full flex-col', dark ? 'bg-[#0F1117] text-[#E8EDF5]' : 'bg-[#F5F7FA] text-[#1A2030]')}>
      <div className="flex flex-1 flex-col px-5 pb-5 pt-6">
        <div className="mb-4 flex flex-col items-center text-center">
          <AppIcon size={40} dark={dark} />
          <h1 className="mt-3 mb-1 text-[18px] font-bold">连接另一台电脑</h1>
          <p className={cn('m-0 max-w-[300px] text-[12px] leading-5', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
            两台电脑需在同一局域网。配对后即可跨设备复制粘贴。
          </p>
        </div>

        <div
          className={cn(
            'mb-4 flex items-center gap-3 rounded-[10px] border px-3 py-3',
            dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white',
          )}
        >
          <div className={cn('flex size-9 items-center justify-center rounded-lg', dark ? 'bg-[#252D3D]' : 'bg-[#EEF2F8]')}>
            <Monitor size={16} className="text-primary" />
          </div>
          <div>
            <div className="text-[12px] font-semibold">{mockDevices.local.name}</div>
            <div className={cn('text-[11px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
              本机 · {mockDevices.local.os}
            </div>
          </div>
        </div>

        <div
          className={cn(
            'mb-4 rounded-[12px] border p-4 text-center',
            dark ? 'border-white/10 bg-[#1C2030]' : 'border-black/8 bg-white',
          )}
        >
          <div className={cn('mb-2 text-[11px] font-medium tracking-wide uppercase', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
            配对码
          </div>
          <div className="mb-3 font-mono text-[28px] font-bold tracking-[0.18em] text-primary">{pairCode}</div>
          <div className="flex justify-center gap-2">
            <button
              type="button"
              className={cn(
                'inline-flex items-center gap-1 rounded-md px-2.5 py-1.5 text-[12px] font-medium',
                dark ? 'bg-[#252D3D] text-[#E8EDF5]' : 'bg-[#F1F4F7] text-[#1A2030]',
              )}
            >
              <Copy size={12} /> 复制
            </button>
            <button
              type="button"
              className={cn(
                'inline-flex items-center gap-1 rounded-md px-2.5 py-1.5 text-[12px] font-medium',
                dark ? 'bg-[#252D3D] text-[#E8EDF5]' : 'bg-[#F1F4F7] text-[#1A2030]',
              )}
            >
              <RefreshCw size={12} /> 刷新
            </button>
          </div>
        </div>

        <div className={cn('mb-4 text-center text-[12px]', dark ? 'text-[#8896AC]' : 'text-[#6B7589]')}>
          {state === 'waiting' && '正在等待另一台电脑…'}
          {state === 'pairing' && '正在配对…'}
          {state === 'success' && '配对成功'}
          {state === 'error' && <span className="text-destructive">配对失败，请重试</span>}
        </div>

        <PrimaryButton loading={state === 'pairing'}>已在另一台设备确认</PrimaryButton>
        <PrimaryButton variant="ghost" className="mt-2">
          手动输入 IP / 配对码
        </PrimaryButton>

        <div className={cn('mt-auto flex items-start gap-2 pt-4 text-[11px] leading-4', dark ? 'text-[#5A6680]' : 'text-[#9AA3B2]')}>
          <Info size={12} className="mt-0.5 shrink-0" />
          <span>需同一局域网，并允许本应用通过防火墙。查看如何使用</span>
        </div>
      </div>
    </div>
  )
}
