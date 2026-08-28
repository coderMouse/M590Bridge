import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Copy,
  Link2,
  RefreshCw,
  Settings,
  WifiOff,
  Monitor,
  Info,
  FileUp,
  FolderOpen,
  X,
} from 'lucide-react'
import { AppIcon } from '@/components/AppIcon'
import { PrimaryButton } from '@/components/PrimaryButton'
import { StatusPill } from '@/components/StatusPill'
import { DeviceCard } from '@/components/DeviceCard'
import { ClipboardPreview } from '@/components/ClipboardPreview'
import { Toggle } from '@/components/Toggle'
import { cn } from '@/lib/cn'
import {
  bytesToBase64,
  batchProgressPercent,
  fetchAutostartEnabled,
  fetchConfig,
  fetchDiscover,
  postDiscoverRefresh,
  fetchHubRuntimeInfo,
  fetchStatus,
  filePhaseLabel,
  fileProgressPercent,
  formatBytes,
  getApiBase,
  hubOfflineMessage,
  MAX_SEND_FILE_BYTES,
  phaseToStatusLabel,
  postConfig,
  postCancelBatch,
  postConnect,
  postDisconnect,
  postListen,
  postPush,
  postSendBatch,
  postSendFileBytes,
  pickSendFilesNative,
  pickSendFolderNative,
  resolveHubOfflineReason,
  setAutostartEnabled,
  isDesktopAutostartShell,
  isTauriShell,
  randomPairCode,
  type DiscoveredPeer,
  type HubOfflineReason,
  type HubStatus,
} from '@/lib/bridgeApi'
import type { ConnectionStatus } from '@/lib/tokens'

type Tab = 'pair' | 'home' | 'settings'

const TAB_ITEMS = [
  { id: 'pair', label: '配对', icon: Link2 },
  { id: 'home', label: '主面板', icon: Monitor },
  { id: 'settings', label: '设置', icon: Settings },
] as const

function formatCodeDisplay(code: string) {
  const digits = code.replace(/\D/g, '').slice(0, 6)
  if (digits.length <= 3) return digits
  return `${digits.slice(0, 3)} ${digits.slice(3)}`
}

export function OperableApp({ onOpenGallery }: { onOpenGallery?: () => void }) {
  const [tab, setTab] = useState<Tab>('pair')
  const [hubOnline, setHubOnline] = useState(false)
  const [hubOfflineReason, setHubOfflineReason] = useState<HubOfflineReason>('starting')
  const [hubRuntimeError, setHubRuntimeError] = useState<string | null>(null)
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
  const [autostartEnabled, setAutostartEnabledState] = useState(false)
  const [autostartBusy, setAutostartBusy] = useState(false)
  const [prefsLoaded, setPrefsLoaded] = useState(false)
  const [settingsDeviceId, setSettingsDeviceId] = useState('')
  const [settingsPort, setSettingsPort] = useState(5901)
  const [settingsAddr, setSettingsAddr] = useState('127.0.0.1:5901')
  const [settingsCode, setSettingsCode] = useState('')
  const [settingsSaved, setSettingsSaved] = useState<string | null>(null)
  const [settingsFileSaveDir, setSettingsFileSaveDir] = useState('')
  const [fileBusy, setFileBusy] = useState(false)
  const [fileDragOver, setFileDragOver] = useState(false)
  const [pickedFileLabel, setPickedFileLabel] = useState<string | null>(null)
  const [discoveredPeers, setDiscoveredPeers] = useState<DiscoveredPeer[]>([])
  const [discoverError, setDiscoverError] = useState<string | null>(null)
  const [discoverBusy, setDiscoverBusy] = useState(false)
  const fileInputRef = useRef<HTMLInputElement | null>(null)

  const apiBase = useMemo(() => getApiBase(), [])
  const tauriShell = useMemo(() => isTauriShell(), [])
  const autostartSupported = useMemo(() => isDesktopAutostartShell(), [])

  const refresh = useCallback(async () => {
    const reason = await resolveHubOfflineReason()
    const ok = reason === 'online'
    setHubOnline(ok)
    setHubOfflineReason(reason)
    if (!ok) {
      const runtime = await fetchHubRuntimeInfo()
      setHubRuntimeError(runtime?.error ?? null)
      setStatus(null)
      return
    }
    setHubRuntimeError(null)
    try {
      const s = await fetchStatus()
      setStatus(s)
      setError(s.last_error)
      setAutoSync(s.auto_sync)
      setAutoReconnect(s.auto_reconnect)
      if (s.device_id) {
        setSettingsDeviceId((prev) => prev || s.device_id)
      }
      if (s.file_save_dir) {
        setSettingsFileSaveDir((prev) => prev || s.file_save_dir || '')
      }
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
    if (!hubOnline || role !== 'joiner' || tab !== 'pair') {
      return
    }
    let cancelled = false
    const poll = async () => {
      try {
        const d = await fetchDiscover()
        if (cancelled) return
        setDiscoveredPeers(Array.isArray(d.peers) ? d.peers : [])
        setDiscoverError(d.error ?? null)
      } catch (e) {
        if (!cancelled) {
          setDiscoverError(e instanceof Error ? e.message : String(e))
        }
      }
    }
    void poll()
    const id = window.setInterval(() => void poll(), 2000)
    return () => {
      cancelled = true
      window.clearInterval(id)
    }
  }, [hubOnline, role, tab])

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
        setSettingsDeviceId(cfg.device_id || '')
        setSettingsPort(cfg.listen_port || 5901)
        setSettingsAddr(cfg.connect_addr || '127.0.0.1:5901')
        setSettingsCode((cfg.pairing_code || '').replace(/\D/g, '').slice(0, 6))
        setAutoSync(cfg.auto_sync)
        setAutoReconnect(cfg.auto_reconnect)
        if (cfg.file_save_dir) setSettingsFileSaveDir(cfg.file_save_dir)
        setPrefsLoaded(true)
      } catch {
        setPrefsLoaded(true)
      }
    })()
  }, [hubOnline, prefsLoaded])

  useEffect(() => {
    if (!autostartSupported) return
    let cancelled = false
    void fetchAutostartEnabled()
      .then((enabled) => {
        if (!cancelled) setAutostartEnabledState(enabled)
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e))
      })
    return () => {
      cancelled = true
    }
  }, [autostartSupported])

  const connLabel: ConnectionStatus = status
    ? (phaseToStatusLabel(status.phase, status.connection) as ConnectionStatus)
    : '未连接'
  const batchActive = Boolean(
    status?.file_batch_id &&
      ['offered', 'sending', 'receiving'].includes(status.file_transfer_phase ?? ''),
  )

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
      if (role === 'host') {
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

  async function onRefreshDiscover() {
    if (!hubOnline || discoverBusy) return
    setDiscoverBusy(true)
    setDiscoverError(null)
    try {
      const d = await postDiscoverRefresh()
      setDiscoveredPeers(Array.isArray(d.peers) ? d.peers : [])
      setDiscoverError(d.error ?? null)
      // Give mDNS a moment, then pull again.
      window.setTimeout(() => {
        void fetchDiscover()
          .then((again) => {
            setDiscoveredPeers(Array.isArray(again.peers) ? again.peers : [])
            setDiscoverError(again.error ?? null)
          })
          .catch(() => {})
      }, 600)
    } catch (e) {
      setDiscoverError(e instanceof Error ? e.message : String(e))
    } finally {
      setDiscoverBusy(false)
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

  async function onPickAndSendBrowserFiles(files: File[]) {
    if (files.length === 0) return
    if (files.length > 1) {
      setError('浏览器模式不支持批次路径传输；请使用桌面版原生多选或拖放')
      return
    }
    const file = files[0]
    setError(null)
    setPickedFileLabel(`${file.name} (${file.size}B)`)
    if (file.size > MAX_SEND_FILE_BYTES) {
      setError('浏览器模式单文件上限 4MiB；请使用桌面版原生选择或拖放发送大文件')
      return
    }
    // Basename only for wire protocol safety.
    const baseName = file.name.split(/[/\\]/).pop() || file.name
    if (!baseName || baseName.includes('..')) {
      setError('无效文件名')
      return
    }
    setFileBusy(true)
    try {
      const buf = new Uint8Array(await file.arrayBuffer())
      const data_base64 = bytesToBase64(buf)
      await postSendFileBytes({ name: baseName, data_base64 })
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setFileBusy(false)
    }
  }

  async function onNativePickAndSendFiles() {
    setError(null)
    setFileBusy(true)
    try {
      if (isTauriShell()) {
        const paths = await pickSendFilesNative()
        if (paths.length === 0) return
        setPickedFileLabel(
          paths.length === 1
            ? paths[0].split(/[/\\]/).pop() || paths[0]
            : `${paths.length} 个文件`,
        )
        await postSendBatch(paths)
        await refresh()
        return
      }
      fileInputRef.current?.click()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setFileBusy(false)
    }
  }

  async function onNativePickAndSendFolder() {
    setError(null)
    setFileBusy(true)
    try {
      if (!isTauriShell()) {
        throw new Error('文件夹批次发送需要桌面版')
      }
      const path = await pickSendFolderNative()
      if (!path) return
      setPickedFileLabel(path.split(/[/\\]/).pop() || path)
      await postSendBatch([path])
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setFileBusy(false)
    }
  }

  async function onCancelBatch() {
    setError(null)
    setFileBusy(true)
    try {
      await postCancelBatch()
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setFileBusy(false)
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

  async function onToggleAutostart(next: boolean) {
    if (autostartBusy) return
    const previous = autostartEnabled
    setAutostartEnabledState(next)
    setAutostartBusy(true)
    setSettingsSaved(null)
    setError(null)
    try {
      const enabled = await setAutostartEnabled(next)
      setAutostartEnabledState(enabled)
      if (enabled !== next) {
        throw new Error('当前平台不支持登录自启')
      }
      setSettingsSaved(enabled ? '登录自启已开启' : '登录自启已关闭')
    } catch (e) {
      setAutostartEnabledState(previous)
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setAutostartBusy(false)
    }
  }

  async function onSaveSettings() {
    setBusy(true)
    setSettingsSaved(null)
    setError(null)
    try {
      const cleanCode = settingsCode.replace(/\D/g, '').slice(0, 6)
      const listenPort = Number.isFinite(settingsPort) && settingsPort > 0 ? settingsPort : 5901
      const peerAddr = settingsAddr.trim()
      const deviceId = settingsDeviceId.trim()
      await postConfig({
        device_id: deviceId || undefined,
        listen_port: listenPort,
        connect_addr: peerAddr || null,
        pairing_code: cleanCode || null,
        last_role: role,
        auto_sync: autoSync,
        auto_reconnect: autoReconnect,
        file_save_dir: settingsFileSaveDir.trim() || undefined,
      })
      // Keep pair tab fields in sync with saved defaults
      setPort(listenPort)
      if (peerAddr) setAddr(peerAddr)
      if (cleanCode) setCode(cleanCode)
      await refresh()
      setSettingsSaved('已保存到本机配置')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  function phaseLabel(phase?: string | null) {
    switch (phase) {
      case 'idle':
        return '空闲'
      case 'waiting_peer':
        return '等待对端'
      case 'pairing':
        return '配对中'
      case 'connected':
        return '已连接'
      case 'error':
        return '错误'
      default:
        return phase || '未知'
    }
  }

  return (
    <div className="flex h-dvh min-h-0 w-full min-w-0 flex-col overflow-hidden bg-[#F5F7FA] text-[#1A2030]">
      <header className="flex shrink-0 items-center justify-between border-b border-black/6 bg-white px-4 py-3">
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
          {tauriShell ? (
            <>{hubOfflineMessage(hubOfflineReason, hubRuntimeError)}</>
          ) : (
            <>
              未检测到本机 hub。请先在仓库根目录运行：
              <pre className="mt-2 overflow-x-auto rounded bg-white/80 p-2 text-[11px]">
                cargo run -p m590-daemon -- hub --api 127.0.0.1:5910
              </pre>
              然后刷新本页。可用 <code>?api=http://127.0.0.1:5910</code> 指定地址。
            </>
          )}
        </div>
      ) : null}

      {error ? (
        <div className="mx-4 mt-3 flex items-center gap-2 rounded-lg bg-status-error-bg px-3 py-2 text-[12px] text-status-error">
          <WifiOff size={14} />
          <span className="flex-1">{error}</span>
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1">
        <aside className="hidden w-40 shrink-0 flex-col border-r border-black/6 bg-white sm:flex">
          <nav className="space-y-1 p-2" aria-label="主导航">
            {TAB_ITEMS.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                className={cn(
                  'flex w-full items-center gap-2 rounded-md px-3 py-2.5 text-left text-[12px] font-semibold transition-colors',
                  tab === id
                    ? 'bg-primary/10 text-primary'
                    : 'text-[#6B7589] hover:bg-[#F5F7FA] hover:text-[#1A2030]',
                )}
                aria-current={tab === id ? 'page' : undefined}
                onClick={() => setTab(id)}
              >
                <Icon size={15} />
                <span>{label}</span>
              </button>
            ))}
          </nav>
        </aside>

        <main className="min-w-0 flex-1 overflow-auto">
          {tab === 'pair' ? (
          <div className="mx-auto flex min-h-full w-full max-w-[720px] flex-col px-5 pb-5 pt-6 sm:px-8">
            <div className="mb-4 flex flex-col items-center text-center">
              <AppIcon size={40} />
              <h1 className="mt-3 mb-1 text-[18px] font-bold">连接另一台电脑</h1>
              <p className="m-0 max-w-[300px] text-[12px] leading-5 text-[#6B7589]">
                两台电脑需在同一局域网。一端「创建配对」，另一端「加入」可从局域网列表点选或手动填 IP。
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
              <div className="mb-3 space-y-2">
                <div className="rounded-[10px] border border-black/8 bg-white p-3">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="text-[12px] font-semibold text-[#6B7589]">局域网设备</span>
                    <div className="flex items-center gap-2">
                      <span className="text-[11px] text-[#9AA3B2]">
                        {discoveredPeers.length > 0
                          ? `${discoveredPeers.length} 台`
                          : discoverError
                            ? '发现不可用'
                            : discoverBusy
                              ? '刷新中…'
                              : '搜索中…'}
                      </span>
                      <button
                        type="button"
                        title="刷新局域网设备"
                        aria-label="刷新局域网设备"
                        disabled={!hubOnline || discoverBusy}
                        onClick={() => void onRefreshDiscover()}
                        className={cn(
                          'inline-flex size-7 items-center justify-center rounded-md border border-black/8 bg-[#F7F9FC] text-[#1A2030]',
                          (!hubOnline || discoverBusy) && 'opacity-50',
                        )}
                      >
                        <RefreshCw
                          size={13}
                          className={cn(discoverBusy && 'animate-spin')}
                        />
                      </button>
                    </div>
                  </div>
                  {discoveredPeers.length === 0 ? (
                    <div className="text-[11px] leading-4 text-[#9AA3B2]">
                      {discoverError
                        ? `mDNS：${discoverError}（仍可手动输入 IP）`
                        : '未发现其他 M590Bridge。请确认对端已点「开始等待配对」，或点刷新。'}
                    </div>
                  ) : (
                    <ul className="m-0 max-h-36 list-none space-y-1 overflow-auto p-0">
                      {discoveredPeers.map((p) => {
                        const selected = addr === p.addr
                        const rowKey = p.device_id || p.addr || p.fullname
                        return (
                          <li key={rowKey}>
                            <button
                              type="button"
                              onClick={() => setAddr(p.addr)}
                              className={cn(
                                'flex w-full items-center justify-between rounded-md px-2.5 py-2 text-left text-[12px]',
                                selected
                                  ? 'bg-primary/10 text-primary'
                                  : 'bg-[#F7F9FC] text-[#1A2030] hover:bg-[#EEF2F8]',
                              )}
                            >
                              <span className="min-w-0 truncate font-medium">
                                {p.device_id || p.name || '设备'}
                              </span>
                              <span className="ml-2 shrink-0 font-mono text-[11px] opacity-80">
                                {p.addr}
                              </span>
                            </button>
                          </li>
                        )
                      })}
                    </ul>
                  )}
                </div>
                <label className="block text-[12px]">
                  <span className="mb-1 block text-[#6B7589]">对端地址 host:port</span>
                  <input
                    className="w-full rounded-lg border border-black/10 bg-white px-3 py-2 font-mono"
                    value={addr}
                    onChange={(e) => setAddr(e.target.value)}
                    placeholder="192.168.1.10:5901"
                  />
                </label>
              </div>
            )}

            <div className="mb-4 text-center text-[12px] text-[#6B7589]">
              {status?.phase === 'waiting_peer' && '正在等待另一台电脑…'}
              {status?.phase === 'pairing' &&
                (status.last_error
                  ? `配对中…（${status.last_error}）`
                  : '正在配对…（约 30 秒超时）')}
              {status?.phase === 'connected' && '配对成功'}
              {status?.phase === 'idle' && '就绪'}
              {status?.phase === 'error' && (
                <span className="text-destructive">失败：{status.last_error ?? '未知错误'}</span>
              )}
            </div>
            {status?.phase === 'error' && status.last_error ? (
              <div className="mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-left text-[11px] leading-4 text-destructive">
                {status.last_error}
              </div>
            ) : null}

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
            <div className="flex-1 overflow-auto">
              <div className="mx-auto grid w-full max-w-[1200px] grid-cols-1 gap-4 px-4 py-4 lg:grid-cols-2 lg:items-start">
                <div className="min-w-0 space-y-4">
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
                </div>

                <div className="min-w-0 space-y-4">
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

                  <div
                    className={
                      fileDragOver
                        ? 'rounded-[10px] border border-primary bg-primary/5 p-3'
                        : 'rounded-[10px] border border-black/8 bg-white p-3'
                    }
                    onDragEnter={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      if (!fileBusy) setFileDragOver(true)
                    }}
                    onDragOver={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      if (!fileBusy) setFileDragOver(true)
                    }}
                    onDragLeave={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      setFileDragOver(false)
                    }}
                    onDrop={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      setFileDragOver(false)
                      if (!hubOnline || status?.phase !== 'connected' || fileBusy || busy) return
                      if (tauriShell) return
                      void onPickAndSendBrowserFiles(Array.from(e.dataTransfer.files ?? []))
                    }}
                  >
                    <div className="mb-2 flex items-center gap-1.5 text-[12px] font-semibold text-[#6B7589]">
                      <FileUp size={14} /> 文件传输
                      <span className="ml-auto font-medium text-[#1A2030]">
                        {filePhaseLabel(status?.file_transfer_phase)}
                      </span>
                    </div>
                    <div className="mb-2 text-[11px] leading-4 text-[#6B7589]">
                      桌面版支持多选文件、选择文件夹和多路径拖放；目录不会跟随符号链接，批次总上限 8GiB。
                      对端按清单顺序逐个接收，全部成功后发布到保存目录。
                      <br />
                      当前条目仍复用单文件流式通道，不会把整个目录读入内存；浏览器模式仅保留单文件 4MiB 发送。
                      <br />
                      两端必须同一版本（含批次通道）。若对端报 unknown message type 16，请升级对端后重连。
                    </div>
                    {status?.file_clipboard_watch_likely === false ? (
                      <div className="mb-2 rounded-md bg-amber-50 px-2 py-1.5 text-[11px] leading-4 text-amber-900">
                        当前环境可能读不到「文件管理器复制」。请用下方按钮或拖入文件发送。
                      </div>
                    ) : null}
                    <input
                      ref={fileInputRef}
                      type="file"
                      multiple
                      className="hidden"
                      disabled={!hubOnline || status?.phase !== 'connected' || fileBusy || busy}
                      onChange={(e) => {
                        void onPickAndSendBrowserFiles(Array.from(e.target.files ?? []))
                        e.target.value = ''
                      }}
                    />
                    <div className="mb-2 grid grid-cols-1 gap-2 sm:grid-cols-2">
                      <PrimaryButton
                        loading={fileBusy}
                        disabled={!hubOnline || status?.phase !== 'connected' || fileBusy || busy}
                        onClick={() => void onNativePickAndSendFiles()}
                      >
                        <FileUp size={14} /> 选择文件（可多选）
                      </PrimaryButton>
                      <PrimaryButton
                        variant="secondary"
                        disabled={!hubOnline || status?.phase !== 'connected' || fileBusy || busy}
                        onClick={() => void onNativePickAndSendFolder()}
                      >
                        <FolderOpen size={14} /> 选择文件夹
                      </PrimaryButton>
                    </div>
                    {pickedFileLabel ? (
                      <div className="mb-2 truncate text-[11px] text-[#6B7589]">已选：{pickedFileLabel}</div>
                    ) : null}
                    <div className="mb-1 flex items-center justify-between text-[11px] text-[#6B7589]">
                      <span className="truncate pr-2">
                        {status?.file_batch_name || status?.last_file_name || '尚无文件传输'}
                      </span>
                      <span className="shrink-0 tabular-nums">{batchProgressPercent(status)}%</span>
                    </div>
                    <div className="mb-2 h-2 overflow-hidden rounded-full bg-[#EEF2F8]">
                      <div
                        className="h-full rounded-full bg-primary transition-[width] duration-300"
                        style={{ width: `${batchProgressPercent(status)}%` }}
                      />
                    </div>
                    <div className="space-y-1 text-[11px] leading-4 text-[#6B7589]">
                      {status?.file_batch_id ? (
                        <>
                          <div>
                            整体：{status.file_batch_files_completed ?? 0} /{' '}
                            {status.file_batch_files_total ?? 0} 个文件 ·{' '}
                            {formatBytes(
                              (status.file_batch_bytes_completed ?? 0) +
                                (status.file_batch_current_path
                                  ? (status.file_bytes_received ?? 0)
                                  : 0),
                            )}{' '}
                            / {formatBytes(status.file_batch_bytes_total ?? 0)}
                          </div>
                          <div className="truncate text-[#1A2030]">
                            当前：{status.file_batch_current_path || '—'}
                          </div>
                          <div className="h-1.5 overflow-hidden rounded-full bg-[#EEF2F8]">
                            <div
                              className="h-full rounded-full bg-emerald-500 transition-[width] duration-300"
                              style={{ width: `${fileProgressPercent(status)}%` }}
                            />
                          </div>
                          <div>
                            当前条目：{formatBytes(status.file_bytes_received ?? 0)} /{' '}
                            {formatBytes(status.file_bytes_total ?? 0)}
                          </div>
                        </>
                      ) : (
                        <div>
                          进度：{formatBytes(status?.file_bytes_received ?? 0)} /{' '}
                          {formatBytes(status?.file_bytes_total ?? 0)}
                        </div>
                      )}
                      {status?.last_file_saved_path ? (
                        <div className="break-all text-[#1A2030]">
                          已保存：{status.last_file_saved_path}
                        </div>
                      ) : null}
                      {status?.file_save_dir ? (
                        <div className="break-all">本机保存目录：{status.file_save_dir}</div>
                      ) : null}
                    </div>
                    {batchActive ? (
                      <PrimaryButton
                        className="mt-2"
                        variant="danger"
                        disabled={fileBusy || busy}
                        onClick={() => void onCancelBatch()}
                      >
                        <X size={14} /> 取消整个批次
                      </PrimaryButton>
                    ) : null}
                    {fileBusy ? (
                      <div className="mt-2 text-[11px] font-medium text-primary">正在提交批次…</div>
                    ) : null}
                  </div>
                </div>
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
          <div className="mx-auto grid w-full max-w-[1200px] grid-cols-1 gap-4 px-4 py-4 text-[13px] lg:grid-cols-2 lg:items-start">
            <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white">
              <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                设备
              </div>
              <div className="space-y-3 px-4 py-3">
                <label className="block">
                  <span className="mb-1 block text-[12px] text-[#6B7589]">本机设备 ID</span>
                  <input
                    className="w-full rounded-lg border border-black/10 bg-[#F8FAFC] px-3 py-2 font-mono text-[12px]"
                    value={settingsDeviceId}
                    onChange={(e) => setSettingsDeviceId(e.target.value)}
                    placeholder="例如 ui-host"
                  />
                </label>
                <div className="flex items-center justify-between gap-3 border-t border-black/5 pt-3 text-[13px]">
                  <span className="text-[#6B7589]">当前对端</span>
                  <span className="max-w-[60%] truncate font-mono text-[12px] text-[#1A2030]">
                    {status?.peer_device || '未连接'}
                  </span>
                </div>
                <div className="flex items-center justify-between gap-3 text-[13px]">
                  <span className="text-[#6B7589]">最近角色</span>
                  <span className="text-[#1A2030]">
                    {status?.role || status?.last_role || role || '未设置'}
                  </span>
                </div>
              </div>
            </section>

            <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white">
              <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                网络
              </div>
              <div className="space-y-3 px-4 py-3">
                <label className="block">
                  <span className="mb-1 block text-[12px] text-[#6B7589]">监听端口（创建配对）</span>
                  <input
                    type="number"
                    className="w-full rounded-lg border border-black/10 bg-[#F8FAFC] px-3 py-2 font-mono text-[12px]"
                    value={settingsPort}
                    onChange={(e) => setSettingsPort(Number(e.target.value) || 5901)}
                  />
                </label>
                <label className="block">
                  <span className="mb-1 block text-[12px] text-[#6B7589]">默认对端地址（加入）</span>
                  <input
                    className="w-full rounded-lg border border-black/10 bg-[#F8FAFC] px-3 py-2 font-mono text-[12px]"
                    value={settingsAddr}
                    onChange={(e) => setSettingsAddr(e.target.value)}
                    placeholder="192.168.1.10:5901"
                  />
                </label>
                <label className="block">
                  <span className="mb-1 block text-[12px] text-[#6B7589]">默认配对码</span>
                  <input
                    className="w-full rounded-lg border border-black/10 bg-[#F8FAFC] px-3 py-2 text-center font-mono text-[16px] tracking-[0.18em] text-primary"
                    value={formatCodeDisplay(settingsCode)}
                    onChange={(e) => setSettingsCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
                    placeholder="123 456"
                  />
                </label>
                <p className="text-[11px] leading-4 text-[#6B7589]">
                  保存后会作为下次启动的默认值；真正开始配对仍在「配对」页操作。
                </p>
              </div>
            </section>

            <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white">
              <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                同步
              </div>
              <div className="space-y-3 px-4 py-3">
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
              </div>
            </section>

            <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white">
              <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                文件
              </div>
              <div className="space-y-3 px-4 py-3">
                <label className="block">
                  <span className="mb-1 block text-[12px] text-[#6B7589]">接收文件保存目录</span>
                  <input
                    className="w-full rounded-lg border border-black/10 bg-[#F8FAFC] px-3 py-2 font-mono text-[12px]"
                    value={settingsFileSaveDir}
                    onChange={(e) => setSettingsFileSaveDir(e.target.value)}
                    placeholder="例如 /home/you/.local/share/m590bridge/inbox"
                  />
                </label>
                <p className="text-[11px] leading-4 text-[#6B7589]">
                  对端发来的文件会写入此目录（自动创建）。路径流式单文件软上限 8GiB。
                </p>
                {status?.last_file_saved_path ? (
                  <div className="break-all text-[11px] text-[#1A2030]">
                    最近落盘：{status.last_file_saved_path}
                  </div>
                ) : null}
              </div>
            </section>

            {autostartSupported ? (
              <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white">
                <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                  启动
                </div>
                <div
                  className={cn(
                    'px-4 py-3',
                    autostartBusy && 'pointer-events-none opacity-60',
                  )}
                >
                  <Toggle
                    label="登录时自动启动"
                    checked={autostartEnabled}
                    onChange={(next) => void onToggleAutostart(next)}
                  />
                </div>
              </section>
            ) : null}

            <section className="overflow-hidden rounded-[12px] border border-black/8 bg-white lg:col-span-2">
              <div className="border-b border-black/5 px-4 py-3 text-[12px] font-semibold text-[#6B7589] uppercase tracking-wide">
                运行状态
              </div>
              <div className="divide-y divide-black/5 text-[13px]">
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">连接阶段</span>
                  <span>{phaseLabel(status?.phase)}</span>
                </div>
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">会话状态</span>
                  <span>{status?.connection || '—'}</span>
                </div>
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">当前端点</span>
                  <span className="max-w-[60%] truncate font-mono text-[12px]">
                    {status?.endpoint || '—'}
                  </span>
                </div>
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">重连次数</span>
                  <span>{status?.reconnect_attempt ?? 0}</span>
                </div>
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">Hub API</span>
                  <span className="max-w-[60%] truncate font-mono text-[11px] text-[#6B7589]">
                    {status?.hub_api || apiBase}
                  </span>
                </div>
                <div className="flex items-center justify-between gap-3 px-4 py-3">
                  <span className="text-[#6B7589]">文件传输</span>
                  <span>{filePhaseLabel(status?.file_transfer_phase)}</span>
                </div>
                {status?.last_error ? (
                  <div className="px-4 py-3">
                    <div className="mb-1 text-[12px] text-[#6B7589]">最近错误</div>
                    <div className="text-[12px] leading-5 text-destructive">{status.last_error}</div>
                  </div>
                ) : null}
                {status?.last_sync_text ? (
                  <div className="px-4 py-3">
                    <div className="mb-1 text-[12px] text-[#6B7589]">最近同步</div>
                    <div className="line-clamp-3 text-[12px] leading-5 text-[#1A2030]">
                      {status.last_sync_text}
                    </div>
                  </div>
                ) : null}
              </div>
            </section>

            <PrimaryButton
              className="lg:col-span-2"
              loading={busy}
              onClick={() => void onSaveSettings()}
              disabled={!hubOnline || busy}
            >
              保存配置
            </PrimaryButton>
            {settingsSaved ? (
              <div className="text-center text-[12px] text-emerald-600 lg:col-span-2">{settingsSaved}</div>
            ) : null}

            <p className="text-center text-[11px] leading-4 text-[#6B7589] lg:col-span-2">
              配置保存在本机。可用环境变量 <code>M590_CONFIG</code> 覆盖路径。
            </p>

            {onOpenGallery ? (
              <PrimaryButton className="lg:col-span-2" variant="ghost" onClick={() => onOpenGallery()}>
                打开设计画廊（对照 Figma）
              </PrimaryButton>
            ) : null}
          </div>
        ) : null}
        </main>
      </div>

      <nav className="flex shrink-0 border-t border-black/6 bg-white text-[12px] font-semibold sm:hidden" aria-label="主导航">
        {TAB_ITEMS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            className={cn(
              'flex min-h-12 flex-1 items-center justify-center gap-1.5 py-2.5',
              tab === id ? 'text-primary' : 'text-[#6B7589]',
            )}
            aria-current={tab === id ? 'page' : undefined}
            onClick={() => setTab(id)}
          >
            <Icon size={14} />
            <span>{label}</span>
          </button>
        ))}
      </nav>
    </div>
  )
}
