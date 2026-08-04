import { useCallback, useEffect, useMemo, useState } from 'react'
import { Copy, Link2, RefreshCw, Settings, WifiOff, Monitor, Info } from 'lucide-react'
import { AppIcon } from '@/components/AppIcon'
import { PrimaryButton } from '@/components/PrimaryButton'
import { StatusPill } from '@/components/StatusPill'
import { DeviceCard } from '@/components/DeviceCard'
import { ClipboardPreview } from '@/components/ClipboardPreview'
import { Toggle } from '@/components/Toggle'
import { cn } from '@/lib/cn'
import {
  fetchConfig,
  fetchHealth,
  fetchStatus,
  getApiBase,
  phaseToStatusLabel,
  postConfig,
  postConnect,
  postDisconnect,
  postListen,
  postPush,
  randomPairCode,
  type HubStatus,
} from '@/lib/bridgeApi'
import type { ConnectionStatus } from '@/lib/tokens'

type Tab = 'pair' | 'home' | 'settings'

function formatCodeDisplay(code: string) {
  const digits = code.replace(/\D/g, '').slice(0, 6)
  if (digits.length <= 3) return digits
  return `${digits.slice(0, 3)} ${digits.slice(3)}`
}

export function OperableApp({ onOpenGallery }: { onOpenGallery?: () => void }) {
  const [tab, setTab] = useState<Tab>('pair')
  const [hubOnline, setHubOnline] = useState(false)
  const [status, setStatus] = useState<HubStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [role, setRole] = useState<'host' | 'joiner'>('host')
  const [code, setCode] = useState(randomPairCode)
  const [port, setPort] = useState(5901)
  const [addr, setAddr] = useState('127.0.0.1:5901')
  const [pushText, setPushText] = useState('')
  const [autoSync, setAutoSync] = useState(true)
  const [autoReconnect, setAutoReconnect] = useState(true)
  const [prefsLoaded, setPrefsLoaded] = useState(false)

  const apiBase = useMemo(() => getApiBase(), [])

  const refresh = useCallback(async () => {
    const ok = await fetchHealth()
    setHubOnline(ok)
    if (!ok) {
      setStatus(null)
      return
    }
    try {
      const s = await fetchStatus()
      setStatus(s)
      setError(s.last_error)
      setAutoSync(s.auto_sync)
      setAutoReconnect(s.auto_reconnect)
      if (s.phase === 'connected') setTab((t) => (t === 'pair' ? 'home' : t))
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    void refresh()
    const id = window.setInterval(() => void refresh(), 1000)
    return () => window.clearInterval(id)
  }, [refresh])

  useEffect(() => {
    if (!hubOnline || prefsLoaded) return
    void (async () => {
      try {
        const cfg = await fetchConfig()
        if (cfg.last_role === 'host' || cfg.last_role === 'joiner') {
          setRole(cfg.last_role)
        }
        if (cfg.pairing_code) setCode(cfg.pairing_code.replace(/\D/g, '').slice(0, 6))
        if (cfg.listen_port) setPort(cfg.listen_port)
        if (cfg.connect_addr) setAddr(cfg.connect_addr)
        setAutoSync(cfg.auto_sync)
        setAutoReconnect(cfg.auto_reconnect)
        setPrefsLoaded(true)
      } catch {
        setPrefsLoaded(true)
      }
    })()
  }, [hubOnline, prefsLoaded])

  const connLabel: ConnectionStatus = status
    ? (phaseToStatusLabel(status.phase, status.connection) as ConnectionStatus)
    : '未连接'

  async function onStart() {
    setBusy(true)
    setError(null)
    try {
      let cleanCode = code.replace(/\D/g, '').slice(0, 6)
      if (cleanCode.length < 4) {
        // UI 状态偶发空码时自动补一个，避免 hub 报 code required
        cleanCode = randomPairCode()
        setCode(cleanCode)
      }
      const listenPort = Number.isFinite(port) && port > 0 ? port : 5901
      const peerAddr = addr.trim() || '127.0.0.1:5901'
      const payload: { code: string; port?: number; addr?: string; device_id?: string } = {
        code: cleanCode,
      }
      if (status?.device_id) payload.device_id = status.device_id
      if (role === 'host') {
        payload.port = listenPort
        await postListen({
          code: cleanCode,
          port: listenPort,
          device_id: status?.device_id,
        })
      } else {
        if (!peerAddr.includes(':')) {
          throw new Error('请填写对端地址，例如 192.168.1.10:5901')
        }
        await postConnect({
          code: cleanCode,
          addr: peerAddr,
          device_id: status?.device_id,
        })
      }
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function onDisconnect() {
    setBusy(true)
    try {
      await postDisconnect()
      await refresh()
      setTab('pair')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function onPush() {
    if (!pushText.trim()) return
    setBusy(true)
    try {
      await postPush(pushText.trim())
      setPushText('')
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function copyCode() {
    try {
      await navigator.clipboard.writeText(code.replace(/\D/g, ''))
    } catch {
      /* ignore */
    }
  }

  async function onToggleAutoSync(next: boolean) {
    setAutoSync(next)
    try {
      await postConfig({ auto_sync: next })
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setAutoSync(!next)
    }
  }

  async function onToggleAutoReconnect(next: boolean) {
    setAutoReconnect(next)
    try {
      await postConfig({ auto_reconnect: next })
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setAutoReconnect(!next)
    }
  }

  return (
    <div className="mx-auto flex min-h-screen max-w-[420px] flex-col bg-[#F5F7FA] text-[#1A2030] shadow-xl">
      <header className="flex items-center justify-between border-b border-black/6 bg-white px-4 py-3">
        <div>
          <div className="text-[14px] font-bold">M590Bridge</div>
          <div className="text-[11px] text-[#6B7589]">
            API {hubOnline ? '已连接' : '未连接'} · {apiBase}
          </div>
        </div>
        <StatusPill status={connLabel} />
      </header>

      {!hubOnline ? (
        <div className="m-4 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] text-amber-900">
          未检测到本机 hub。请先在仓库根目录运行：
          <pre className="mt-2 overflow-x-auto rounded bg-white/80 p-2 text-[11px]">
            cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
          </pre>
          然后刷新本页。可用 <code>?api=http://127.0.0.1:5910</code> 指定地址。
        </div>
      ) : null}

      {error ? (
        <div className="mx-4 mt-3 flex items-center gap-2 rounded-lg bg-status-error-bg px-3 py-2 text-[12px] text-status-error">
          <WifiOff size={14} />
          <span className="flex-1">{error}</span>
        </div>
      ) : null}

      <div className="flex-1 overflow-auto">
        {tab === 'pair' ? (
          <div className="flex flex-col px-5 pb-5 pt-6">
            <div className="mb-4 flex flex-col items-center text-center">
              <AppIcon size={40} />
              <h1 className="mt-3 mb-1 text-[18px] font-bold">连接另一台电脑</h1>
              <p className="m-0 max-w-[300px] text-[12px] leading-5 text-[#6B7589]">
                两台电脑需在同一局域网。一端「创建配对」，另一端「加入」并填写 IP。
              </p>
            </div>

            <div className="mb-4 flex rounded-lg bg-[#EEF2F8] p-1 text-[12px] font-semibold">
              <button
                type="button"
                className={cn('flex-1 rounded-md py-2', role === 'host' && 'bg-white shadow-sm')}
                onClick={() => setRole('host')}
              >
                创建配对（监听）
              </button>
              <button
                type="button"
                className={cn('flex-1 rounded-md py-2', role === 'joiner' && 'bg-white shadow-sm')}
                onClick={() => setRole('joiner')}
              >
                加入（连接）
              </button>
            </div>

            <div className="mb-4 flex items-center gap-3 rounded-[10px] border border-black/8 bg-white px-3 py-3">
              <div className="flex size-9 items-center justify-center rounded-lg bg-[#EEF2F8]">
                <Monitor size={16} className="text-primary" />
              </div>
              <div>
                <div className="text-[12px] font-semibold">{status?.device_id ?? '本机'}</div>
                <div className="text-[11px] text-[#6B7589]">
                  {status?.role ? `角色 · ${status.role}` : '等待开始'}
                </div>
              </div>
            </div>

            <div className="mb-4 rounded-[12px] border border-black/8 bg-white p-4 text-center">
              <div className="mb-2 text-[11px] font-medium tracking-wide text-[#6B7589] uppercase">
                配对码
              </div>
              <input
                className="mb-3 w-full border-0 bg-transparent text-center font-mono text-[28px] font-bold tracking-[0.18em] text-primary outline-none"
                value={formatCodeDisplay(code)}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
              />
              <div className="flex justify-center gap-2">
                <button
                  type="button"
                  onClick={() => void copyCode()}
                  className="inline-flex items-center gap-1 rounded-md bg-[#F1F4F7] px-2.5 py-1.5 text-[12px] font-medium"
                >
                  <Copy size={12} /> 复制
                </button>
                <button
                  type="button"
                  onClick={() => setCode(randomPairCode())}
                  className="inline-flex items-center gap-1 rounded-md bg-[#F1F4F7] px-2.5 py-1.5 text-[12px] font-medium"
                >
                  <RefreshCw size={12} /> 刷新
                </button>
              </div>
            </div>

            {role === 'host' ? (
              <label className="mb-3 block text-[12px]">
                <span className="mb-1 block text-[#6B7589]">监听端口</span>
                <input
                  type="number"
                  className="w-full rounded-lg border border-black/10 bg-white px-3 py-2"
                  value={port}
                  onChange={(e) => setPort(Number(e.target.value) || 5901)}
                />
              </label>
            ) : (
              <label className="mb-3 block text-[12px]">
                <span className="mb-1 block text-[#6B7589]">对端地址 host:port</span>
                <input
                  className="w-full rounded-lg border border-black/10 bg-white px-3 py-2 font-mono"
                  value={addr}
                  onChange={(e) => setAddr(e.target.value)}
                  placeholder="192.168.1.10:5901"
                />
              </label>
            )}

            <div className="mb-4 text-center text-[12px] text-[#6B7589]">
              {status?.phase === 'waiting_peer' && '正在等待另一台电脑…'}
              {status?.phase === 'pairing' && '正在配对…'}
              {status?.phase === 'connected' && '配对成功'}
              {status?.phase === 'idle' && '就绪'}
              {status?.phase === 'error' && (
                <span className="text-destructive">失败：{status.last_error ?? '未知错误'}</span>
              )}
            </div>

            <PrimaryButton
              loading={busy || status?.phase === 'pairing'}
              onClick={() => void onStart()}
              disabled={
                !hubOnline ||
                busy ||
                status?.phase === 'waiting_peer' ||
                status?.phase === 'pairing' ||
                status?.phase === 'connected'
              }
            >
              {role === 'host' ? '开始等待配对' : '连接对端'}
            </PrimaryButton>
            <PrimaryButton
              variant="ghost"
              className="mt-2"
              onClick={() => void onDisconnect()}
              disabled={!hubOnline || busy}
            >
              断开 / 重置
            </PrimaryButton>

            <div className="mt-auto flex items-start gap-2 pt-4 text-[11px] leading-4 text-[#9AA3B2]">
              <Info size={12} className="mt-0.5 shrink-0" />
              <span>需同一局域网并放行防火墙端口。跨机联调可在两端分别选创建/加入。</span>
            </div>
          </div>
        ) : null}

        {tab === 'home' ? (
          <div className="flex h-full flex-col">
            <div className="flex-1 space-y-4 overflow-auto px-4 py-4">
              <div className="flex items-center gap-2">
                <DeviceCard
                  name={status?.device_id ?? '本机'}
                  os={status?.role === 'host' ? 'Host' : 'Joiner'}
                  kind="本机"
                />
                <Link2
                  size={16}
                  className={cn(
                    'shrink-0',
                    status?.phase === 'connected' ? 'text-primary' : 'text-[#9AA3B2]',
                  )}
                />
                <DeviceCard
                  name={status?.peer_device ?? '未连接'}
                  os="对端"
                  kind="对端"
                />
              </div>

              <ClipboardPreview
                type="文本"
                preview={status?.last_sync_text ?? '尚无同步文本'}
                meta={
                  status?.last_sync_content_id
                    ? `content_id ${status.last_sync_content_id}`
                    : '等待同步'
                }
              />

              <div className="rounded-[10px] border border-black/8 bg-white p-3">
                <div className="mb-2 text-[12px] font-semibold text-[#6B7589]">手动推送文本</div>
                <textarea
                  className="mb-2 h-20 w-full rounded-md border border-black/10 p-2 text-[12px]"
                  value={pushText}
                  onChange={(e) => setPushText(e.target.value)}
                  placeholder="输入后推送到对端剪贴板"
                />
                <PrimaryButton
                  onClick={() => void onPush()}
                  disabled={!hubOnline || status?.phase !== 'connected' || busy}
                >
                  推送到对端
                </PrimaryButton>
              </div>
            </div>

            <footer className="space-y-3 border-t border-black/6 bg-white px-4 py-3">
              <Toggle
                label="自动同步剪贴板（hub 侧）"
                checked={autoSync}
                onChange={(v) => void onToggleAutoSync(v)}
              />
              <div className="flex items-center justify-between pt-1">
                <button
                  type="button"
                  className="inline-flex items-center gap-1 text-[12px] font-medium text-primary"
                  onClick={() => setTab('settings')}
                >
                  <Settings size={13} /> 设置
                </button>
                <button
                  type="button"
                  className="inline-flex items-center gap-1 text-[12px] font-medium text-[#6B7589]"
                  onClick={() => void onDisconnect()}
                >
                  断开连接
                </button>
              </div>
            </footer>
          </div>
        ) : null}

        {tab === 'settings' ? (
          <div className="space-y-4 px-4 py-4 text-[13px]">
            <div className="rounded-xl border border-black/8 bg-white p-4 space-y-3">
              <div className="font-semibold">同步与连接</div>
              <Toggle
                label="自动同步剪贴板"
                checked={autoSync}
                onChange={(v) => void onToggleAutoSync(v)}
              />
              <Toggle
                label="断线自动重连"
                checked={autoReconnect}
                onChange={(v) => void onToggleAutoReconnect(v)}
              />
              <p className="text-[12px] text-[#6B7589]">
                重连次数：{status?.reconnect_attempt ?? 0}
                {status?.last_error ? ` · ${status.last_error}` : ''}
              </p>
            </div>
            <div className="rounded-xl border border-black/8 bg-white p-4">
              <div className="mb-2 font-semibold">Hub API</div>
              <div className="break-all font-mono text-[12px] text-[#6B7589]">{apiBase}</div>
              <p className="mt-2 text-[12px] text-[#6B7589]">
                配置保存在本机（可用环境变量 M590_CONFIG 覆盖路径）。桌面壳会内嵌 hub。
              </p>
            </div>
            <div className="rounded-xl border border-black/8 bg-white p-4">
              <div className="mb-2 font-semibold">当前状态</div>
              <pre className="overflow-auto rounded bg-[#F5F7FA] p-2 text-[11px]">
                {JSON.stringify(status, null, 2)}
              </pre>
            </div>
            <PrimaryButton variant="ghost" onClick={() => onOpenGallery?.()}>
              打开设计画廊（对照 Figma）
            </PrimaryButton>
          </div>
        ) : null}
      </div>

      <nav className="flex border-t border-black/6 bg-white text-[12px] font-semibold">
        {(
          [
            ['pair', '配对'],
            ['home', '主面板'],
            ['settings', '设置'],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={cn(
              'flex-1 py-3',
              tab === id ? 'text-primary' : 'text-[#6B7589]',
            )}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>
    </div>
  )
}
