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

const DEFAULT_API = 'http://127.0.0.1:5910'
/** Keep in sync with Session::MAX_FILE_BYTES */
export const MAX_SEND_FILE_BYTES = 4 * 1024 * 1024

export function getApiBase(): string {
  const fromEnv = (import.meta as ImportMeta & { env?: Record<string, string> }).env?.VITE_M590_API
  if (fromEnv && fromEnv.length > 0) return fromEnv.replace(/\/$/, '')
  if (typeof window !== 'undefined') {
    const q = new URLSearchParams(window.location.search).get('api')
    if (q) return q.replace(/\/$/, '')
  }
  return DEFAULT_API
}

async function request<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  const base = getApiBase()
  const res = await fetch(`${base}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
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
