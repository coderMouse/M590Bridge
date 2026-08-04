import { useState } from 'react'
import { AppIcon } from '@/components/AppIcon'
import { Frame, FrameRow, CanvasSectionHeading } from '@/components/Frame'
import { UiKitPanel } from '@/screens/UiKit'
import { PairingScreen } from '@/screens/PairingScreen'
import { HomeScreen } from '@/screens/HomeScreen'
import { TransferScreen } from '@/screens/TransferScreen'
import { SettingsScreen, UnpairModal } from '@/screens/SettingsScreen'
import { TrayMenu, NotificationsSet } from '@/screens/TrayAndNotify'
import { OperableApp } from '@/app/OperableApp'
import { C } from '@/lib/tokens'
import { cn } from '@/lib/cn'

const NAV = [
  { id: 'kit', zh: '组件库', en: 'UI Kit' },
  { id: 'pairing', zh: '配对', en: 'Pairing' },
  { id: 'home', zh: '主面板', en: 'Home' },
  { id: 'transfer', zh: '传输', en: 'Transfer' },
  { id: 'settings', zh: '设置', en: 'Settings' },
  { id: 'tray', zh: '托盘与通知', en: 'Tray & Notify' },
  { id: 'dark', zh: '深色模式', en: 'Dark Mode' },
] as const

type NavId = (typeof NAV)[number]['id']

function DesignGallery({ onBack }: { onBack: () => void }) {
  const [active, setActive] = useState<NavId>('home')

  function renderContent() {
    switch (active) {
      case 'kit':
        return (
          <>
            <CanvasSectionHeading title="01  UI Kit / 组件与色板" subtitle="Figma Make 源 token + 核心组件" />
            <FrameRow>
              <Frame label="UI Kit" width={720} height={900}>
                <UiKitPanel />
              </Frame>
            </FrameRow>
          </>
        )
      case 'pairing':
        return (
          <>
            <CanvasSectionHeading title="02–04  配对 / Pairing" subtitle="等待 · 配对中 · 失败" />
            <FrameRow>
              <Frame label="02  Pairing / 等待对端" width={400} height={560}>
                <PairingScreen state="waiting" />
              </Frame>
              <Frame label="03  Pairing / 配对中" width={400} height={560}>
                <PairingScreen state="pairing" />
              </Frame>
              <Frame label="04  Pairing / 失败" width={400} height={560}>
                <PairingScreen state="error" />
              </Frame>
            </FrameRow>
          </>
        )
      case 'home':
        return (
          <>
            <CanvasSectionHeading title="05–07  主面板 / Home" subtitle="已连接 · 同步中 · 断连" />
            <FrameRow>
              <Frame label="05  Home / 已连接" width={380} height={560}>
                <HomeScreen status="已连接" />
              </Frame>
              <Frame label="06  Home / 同步中" width={380} height={560}>
                <HomeScreen status="同步中" />
              </Frame>
              <Frame label="07  Home / 断连重试" width={380} height={560}>
                <HomeScreen disconnected />
              </Frame>
            </FrameRow>
          </>
        )
      case 'transfer':
        return (
          <>
            <CanvasSectionHeading title="08–09  传输 / Transfer" subtitle="进行中进度卡" />
            <FrameRow>
              <Frame label="08  Transfer / 传输中" width={380} height={480}>
                <TransferScreen />
              </Frame>
            </FrameRow>
          </>
        )
      case 'settings':
        return (
          <>
            <CanvasSectionHeading title="10–11  设置 / Settings" subtitle="分组设置与危险操作确认" />
            <FrameRow>
              <Frame label="10  Settings / 设置" width={420} height={600}>
                <SettingsScreen />
              </Frame>
              <Frame label="11  Unpair / 解除配对确认" canvasBg="#C8CDD6" width={420} height={360}>
                <UnpairModal />
              </Frame>
            </FrameRow>
          </>
        )
      case 'tray':
        return (
          <>
            <CanvasSectionHeading title="12–13  托盘与通知" subtitle="Windows / Linux 菜单 · 通知集" />
            <FrameRow>
              <Frame label="12a  Tray / Windows" canvasBg="#D0D5DE" width={280} height={290}>
                <div className="flex justify-center p-5">
                  <TrayMenu platform="windows" />
                </div>
              </Frame>
              <Frame label="12b  Tray / Linux" canvasBg="#C8CDD6" width={280} height={290}>
                <div className="flex justify-center p-5">
                  <TrayMenu platform="linux" />
                </div>
              </Frame>
              <Frame label="13  Notifications" width={320} height={290}>
                <NotificationsSet />
              </Frame>
            </FrameRow>
          </>
        )
      case 'dark':
        return (
          <>
            <CanvasSectionHeading title="深色模式" subtitle="配对 / 主面板暗色对照" />
            <FrameRow>
              <Frame label="Pairing dark" width={400} height={560} canvasBg="#0B0D12">
                <PairingScreen state="waiting" dark />
              </Frame>
              <Frame label="Home dark" width={380} height={560} canvasBg="#0B0D12">
                <HomeScreen status="已连接" dark />
              </Frame>
            </FrameRow>
          </>
        )
      default:
        return null
    }
  }

  return (
    <div className="min-h-screen" style={{ background: C.bg }}>
      <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-black/10 bg-white/90 px-4 py-3 backdrop-blur">
        <AppIcon size={28} />
        <div className="flex-1">
          <div className="text-sm font-bold">M590Bridge · 设计画廊</div>
          <div className="text-xs text-[#6B7589]">只读对照 Figma，不驱动 daemon</div>
        </div>
        <button
          type="button"
          className="rounded-lg bg-primary px-3 py-1.5 text-xs font-semibold text-white"
          onClick={onBack}
        >
          返回可操作壳
        </button>
      </header>
      <div className="flex gap-2 overflow-x-auto px-4 py-3">
        {NAV.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setActive(item.id)}
            className={cn(
              'shrink-0 rounded-full px-3 py-1.5 text-xs font-semibold',
              active === item.id ? 'bg-primary text-white' : 'bg-white text-[#6B7589]',
            )}
          >
            {item.zh}
          </button>
        ))}
      </div>
      <main className="px-4 pb-10">{renderContent()}</main>
    </div>
  )
}

export default function App() {
  const [mode, setMode] = useState<'app' | 'gallery'>('app')
  if (mode === 'gallery') {
    return <DesignGallery onBack={() => setMode('app')} />
  }
  return <OperableApp onOpenGallery={() => setMode('gallery')} />
}
