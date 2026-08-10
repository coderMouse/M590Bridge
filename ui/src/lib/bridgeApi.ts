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

let hubTokenPromise: Promise<string | null> | null = null

async function getHubAuthToken(): Promise<string | null> {
  if (hubTokenPromise) return hubTokenPromise
  hubTokenPromise = (async () => {
    const invoke = getTauriInvoke()
    if (invoke) {
      const token = await invoke('hub_auth_token')
      return typeof token === 'string' && token.length >= 32 ? token : null
    }
    const env = (import.meta as ImportMeta & {
      env?: { DEV?: boolean; VITE_M590_HUB_TOKEN?: string }
    }).env
    const envToken = env?.DEV ? env.VITE_M590_HUB_TOKEN?.trim() : null
    return envToken && envToken.length >= 32 ? envToken : null
  })().catch(() => null)
  return hubTokenPromise
}

async function request<T = unknown>(path: string, init?: RequestInit): Promise<T> {
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
  let data: unknown = null
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    data = { raw: text }
  }
  if (!res.ok) {
    const errMsg =
      data && typeof data === 'object' && data !== null && 'error' in data
        ? String((data as { error: unknown }).error)
        : res.statusText
    throw new Error(errMsg || `HTTP ${res.status}`)
  }
  return data as T
}

export async function fetchHealth(): Promise<boolean> {
  try {
    await request('/api/health')
    return true
  } catch {
    return false
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
