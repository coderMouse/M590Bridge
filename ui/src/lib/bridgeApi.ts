/** Localhost hub API client for the operable UI shell. */

export type HubStatus = {
  phase: 'idle' | 'waiting_peer' | 'pairing' | 'connected' | 'error' | string
  role: string | null
  device_id: string
  peer_device: string | null
  pairing_code: string | null
  endpoint: string | null
  connection: string | null
  last_sync_text: string | null
  last_sync_content_id: string | null
  last_error: string | null
  auto_sync: boolean
  auto_reconnect: boolean
  reconnect_attempt: number
  last_role: string | null
  listen_port: number
  connect_addr: string | null
  hub_api: string | null
  file_save_dir?: string
  file_transfer_phase?: string | null
  last_file_transfer_id?: string | null
  last_file_name?: string | null
  last_file_bytes?: number | null
  last_file_saved_path?: string | null
  file_bytes_received?: number | null
  file_bytes_total?: number | null
  /** false: OS file-manager copy may not be visible (e.g. GNOME Wayland). */
  file_clipboard_watch_likely?: boolean
}

export type HubConfig = {
  device_id: string
  last_role: string | null
  pairing_code: string | null
  listen_port: number
  connect_addr: string | null
  auto_sync: boolean
  auto_reconnect: boolean
  file_save_dir?: string
}

export type DiscoveredPeer = {
  name: string
  device_id: string
  host: string
  port: number
  addr: string
  fullname: string
  last_seen_unix_ms: number
}

export type DiscoverResponse = {
  service_type: string
  advertising: boolean
  peers: DiscoveredPeer[]
  error?: string
}

const DEFAULT_API = 'http://127.0.0.1:5910'
/** Browser File/base64 fallback cap; native desktop path sends use the 8 GiB core soft cap. */
export const MAX_SEND_FILE_BYTES = 4 * 1024 * 1024

type TauriInvoke = (cmd: string, args?: object) => Promise<unknown>

function getTauriInvoke(): TauriInvoke | null {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: TauriInvoke }
    __TAURI__?: { core?: { invoke: TauriInvoke } }
  }
  return w.__TAURI_INTERNALS__?.invoke ?? w.__TAURI__?.core?.invoke ?? null
}

function loopbackApi(value: string | undefined): string | null {
  if (!value) return null
  try {
    const url = new URL(value)
    if (!['http:', 'https:'].includes(url.protocol)) return null
    if (!['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname)) return null
    return url.origin
  } catch {
    return null
  }
}

export function getApiBase(): string {
  const fromEnv = (import.meta as ImportMeta & { env?: Record<string, string> }).env?.VITE_M590_API
  const envApi = loopbackApi(fromEnv)
  if (envApi) return envApi
  if (typeof window !== 'undefined') {
    const q = new URLSearchParams(window.location.search).get('api')
    const queryApi = loopbackApi(q ?? undefined)
    if (queryApi) return queryApi
  }
  return DEFAULT_API
}

let cachedHubToken: string | null = null
let hubTokenInflight: Promise<string | null> | null = null

async function getHubAuthToken(): Promise<string | null> {
  if (cachedHubToken) return cachedHubToken
  if (hubTokenInflight) return hubTokenInflight
  hubTokenInflight = (async () => {
    try {
      const invoke = getTauriInvoke()
      if (invoke) {
        const token = await invoke('hub_auth_token')
        if (typeof token === 'string' && token.length >= 32) {
          cachedHubToken = token
          return cachedHubToken
        }
        return null
      }
      const env = (import.meta as ImportMeta & {
        env?: { DEV?: boolean; VITE_M590_HUB_TOKEN?: string }
      }).env
      const envToken = env?.DEV ? env.VITE_M590_HUB_TOKEN?.trim() : null
      if (envToken && envToken.length >= 32) {
        cachedHubToken = envToken
        return cachedHubToken
      }
      return null
    } catch {
      return null
    } finally {
      hubTokenInflight = null
    }
  })()
  return hubTokenInflight
}

export type HubRuntimeInfo = {
  ready: boolean
  error: string | null
  api: string
}

export type HubOfflineReason =
  | 'online'
  | 'starting'
  | 'unreachable'
  | 'token_unavailable'
  | 'unauthorized'
  | 'origin_denied'
  | 'port_in_use'
  | 'start_failed'
  | 'http_error'

export async function fetchHubRuntimeInfo(): Promise<HubRuntimeInfo | null> {
  const invoke = getTauriInvoke()
  if (!invoke) return null
  try {
    const info = await invoke('hub_runtime_info')
    if (!info || typeof info !== 'object') return null
    const rec = info as { ready?: unknown; error?: unknown; api?: unknown }
    return {
      ready: Boolean(rec.ready),
      error: typeof rec.error === 'string' ? rec.error : rec.error == null ? null : String(rec.error),
      api: typeof rec.api === 'string' ? rec.api : DEFAULT_API,
    }
  } catch {
    return null
  }
}

type HubApiProxyResponse = {
  status: number
  body: string
}

function parseHubBody(text: string): unknown {
  if (!text) return null
  try {
    return JSON.parse(text)
  } catch {
    return { raw: text }
  }
}

function hubErrorMessage(data: unknown, statusText: string, status: number): string {
  if (data && typeof data === 'object' && data !== null && 'error' in data) {
    return String((data as { error: unknown }).error)
  }
  return statusText || `HTTP ${status}`
}

async function requestViaTauri<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  const invoke = getTauriInvoke()
  if (!invoke) throw new Error('Tauri invoke unavailable')
  const method = (init?.method ?? 'GET').toUpperCase()
  const body =
    typeof init?.body === 'string'
      ? init.body
      : init?.body == null
        ? ''
        : String(init.body)
  const response = (await invoke('hub_api_request', {
    args: {
      method,
      path,
      body,
    },
  })) as HubApiProxyResponse
  const text = typeof response?.body === 'string' ? response.body : ''
  const status = typeof response?.status === 'number' ? response.status : 0
  const data = parseHubBody(text)
  if (status < 200 || status >= 300) {
    throw new Error(hubErrorMessage(data, '', status))
  }
  return data as T
}

async function requestViaFetch<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  const base = getApiBase()
  const authToken = await getHubAuthToken()
  if (!authToken) throw new Error('Hub authentication token unavailable')
  const res = await fetch(`${base}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      'X-M590-Token': authToken,
      ...(init?.headers ?? {}),
    },
  })
  const text = await res.text()
  const data = parseHubBody(text)
  if (!res.ok) {
    throw new Error(hubErrorMessage(data, res.statusText, res.status))
  }
  return data as T
}

async function request<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  if (getTauriInvoke()) {
    return requestViaTauri<T>(path, init)
  }
  return requestViaFetch<T>(path, init)
}

export async function fetchHealth(): Promise<boolean> {
  const result = await probeHubHealth()
  return result === 'online'
}

export async function probeHubHealth(): Promise<HubOfflineReason> {
  try {
    if (getTauriInvoke()) {
      // Token is injected by the Rust proxy; only ensure the command is reachable.
      const token = await getHubAuthToken()
      if (!token) return 'token_unavailable'
      await requestViaTauri('/api/health', { method: 'GET' })
      return 'online'
    }
    const authToken = await getHubAuthToken()
    if (!authToken) return 'token_unavailable'
    const base = getApiBase()
    const res = await fetch(`${base}/api/health`, {
      headers: {
        'Content-Type': 'application/json',
        'X-M590-Token': authToken,
      },
    })
    if (res.ok) return 'online'
    if (res.status === 401) return 'unauthorized'
    if (res.status === 403) return 'origin_denied'
    return 'http_error'
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    const lower = msg.toLowerCase()
    if (lower.includes('hub authentication required') || lower.includes('unauthorized')) {
      return 'unauthorized'
    }
    if (lower.includes('origin not allowed')) {
      return 'origin_denied'
    }
    if (lower.includes('hub connect failed') || lower.includes('connection refused')) {
      return 'unreachable'
    }
    // Tauri proxy path: surface generic transport failures as unreachable.
    return 'unreachable'
  }
}

export async function resolveHubOfflineReason(): Promise<HubOfflineReason> {
  const health = await probeHubHealth()
  if (health === 'online') return 'online'
  if (health !== 'unreachable') return health

  const runtime = await fetchHubRuntimeInfo()
  if (runtime?.error) {
    const err = runtime.error.toLowerCase()
    if (err.includes('address already in use') || err.includes('addrinuse')) {
      return 'port_in_use'
    }
    return 'start_failed'
  }
  if (runtime && !runtime.ready) return 'starting'
  return 'unreachable'
}

export function hubOfflineMessage(reason: HubOfflineReason, runtimeError?: string | null): string {
  switch (reason) {
    case 'port_in_use':
      return '内嵌 Hub 端口 5910 被占用。请退出重复的 M590Bridge/m590-daemon 进程后重新打开。'
    case 'start_failed':
      return `内嵌 Hub 启动失败：${runtimeError || '未知错误'}。请重启 M590Bridge。`
    case 'token_unavailable':
      return '无法获取内嵌 Hub 鉴权令牌。请重启 M590Bridge。'
    case 'unauthorized':
      return '内嵌 Hub 鉴权失败（可能连到了其它进程）。请退出全部 M590Bridge 后只打开一个。'
    case 'origin_denied':
      return '内嵌 Hub 拒绝了当前页面来源。请使用正式/standalone 桌面端，不要混用异常入口。'
    case 'http_error':
      return '内嵌 Hub 响应异常。请重启 M590Bridge。'
    case 'unreachable':
      return '内嵌 Hub 仍不可达。请确认未混用开发壳，并退出重复进程后重新打开正式/standalone 桌面端。'
    case 'starting':
    default:
      return '内嵌 Hub 正在启动…'
  }
}

export async function fetchStatus(): Promise<HubStatus> {
  return request<HubStatus>('/api/status')
}

export async function fetchConfig(): Promise<HubConfig> {
  return request<HubConfig>('/api/config')
}

export async function fetchDiscover(): Promise<DiscoverResponse> {
  return request<DiscoverResponse>('/api/discover')
}

export async function postDiscoverRefresh(): Promise<DiscoverResponse> {
  return request<DiscoverResponse>('/api/discover/refresh', {
    method: 'POST',
    body: '{}',
  })
}

export type HubConfigPatch = {
  device_id?: string
  last_role?: string | null
  pairing_code?: string | null
  listen_port?: number
  connect_addr?: string | null
  auto_sync?: boolean
  auto_reconnect?: boolean
  file_save_dir?: string
}

export async function postConfig(partial: HubConfigPatch): Promise<HubConfig> {
  return request<HubConfig>('/api/config', {
    method: 'POST',
    body: JSON.stringify(partial),
  })
}

export async function postListen(input: {
  code: string
  port: number
  device_id?: string
}): Promise<void> {
  const body: Record<string, string | number> = {
    code: String(input.code ?? '').replace(/\D/g, '').slice(0, 6),
    port: Number(input.port) || 5901,
  }
  if (input.device_id) body.device_id = input.device_id
  await request('/api/listen', { method: 'POST', body: JSON.stringify(body) })
}

export async function postConnect(input: {
  code: string
  addr: string
  device_id?: string
}): Promise<void> {
  const body: Record<string, string | number> = {
    code: String(input.code ?? '').replace(/\D/g, '').slice(0, 6),
    addr: String(input.addr ?? '').trim(),
  }
  if (input.device_id) body.device_id = input.device_id
  await request('/api/connect', { method: 'POST', body: JSON.stringify(body) })
}

export async function postPush(text: string): Promise<void> {
  await request('/api/push', { method: 'POST', body: JSON.stringify({ text }) })
}

export async function postSendFile(path: string): Promise<void> {
  await request('/api/send_file', { method: 'POST', body: JSON.stringify({ path }) })
}

export async function postSendFileBytes(input: {
  name: string
  data_base64: string
}): Promise<void> {
  await request('/api/send_file_bytes', {
    method: 'POST',
    body: JSON.stringify({
      name: input.name,
      data_base64: input.data_base64,
    }),
  })
}

/** Encode bytes to standard base64 without blowing the call stack on multi-MB files. */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = ''
  const chunk = 0x8000
  for (let i = 0; i < bytes.length; i += chunk) {
    const slice = bytes.subarray(i, i + chunk)
    binary += String.fromCharCode(...slice)
  }
  return btoa(binary)
}

export async function postDisconnect(): Promise<void> {
  await request('/api/disconnect', { method: 'POST', body: '{}' })
}

export function randomPairCode(): string {
  const n = Math.floor(100000 + Math.random() * 900000)
  return String(n)
}

export function phaseToStatusLabel(phase: string, connection: string | null): string {
  if (phase === 'connected' || connection === 'Connected') return '已连接'
  if (phase === 'pairing' || connection === 'Pairing') return '同步中'
  if (phase === 'waiting_peer') return '未连接'
  if (phase === 'error') return '未连接'
  return '未连接'
}

export function filePhaseLabel(phase: string | null | undefined): string {
  switch (phase) {
    case 'offered':
      return '收到报价'
    case 'sending':
      return '发送中'
    case 'receiving':
      return '接收中'
    case 'done':
      return '已完成'
    case 'failed':
      return '失败'
    default:
      return '空闲'
  }
}

export function fileProgressPercent(status: HubStatus | null): number {
  if (!status) return 0
  const total = status.file_bytes_total ?? 0
  if (total <= 0) {
    if (status.file_transfer_phase === 'done') return 100
    return 0
  }
  const got = status.file_bytes_received ?? 0
  return Math.max(0, Math.min(100, Math.round((got / total) * 100)))
}

/** Tauri native file dialog → absolute path. null if cancelled / not in Tauri. */
export async function pickSendFileNative(): Promise<string | null> {
  const invoke = getTauriInvoke()
  if (!invoke) return null
  const path = (await invoke('pick_send_file')) as string | null
  return path ?? null
}

export async function fetchAutostartEnabled(): Promise<boolean> {
  const invoke = getTauriInvoke()
  if (!invoke) return false
  return (await invoke('autostart_enabled')) === true
}

export async function setAutostartEnabled(enabled: boolean): Promise<boolean> {
  const invoke = getTauriInvoke()
  if (!invoke) return false
  return (await invoke('set_autostart', { enabled })) === true
}

export function isTauriShell(): boolean {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: unknown
    __TAURI__?: unknown
  }
  return Boolean(w.__TAURI_INTERNALS__ || w.__TAURI__)
}

export function isLinuxTauriShell(): boolean {
  if (!isTauriShell() || typeof navigator === 'undefined') return false
  return /Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent)
}
