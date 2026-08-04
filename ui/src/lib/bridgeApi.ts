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
}

export type HubConfig = {
  device_id: string
  last_role: string | null
  pairing_code: string | null
  listen_port: number
  connect_addr: string | null
  auto_sync: boolean
  auto_reconnect: boolean
}

const DEFAULT_API = 'http://127.0.0.1:5910'

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

export async function postConfig( partial: Partial<HubConfig>): Promise<HubConfig> {
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
  await request('/api/listen', { method: 'POST', body: JSON.stringify(input) })
}

export async function postConnect(input: {
  code: string
  addr: string
  device_id?: string
}): Promise<void> {
  await request('/api/connect', { method: 'POST', body: JSON.stringify(input) })
}

export async function postPush(text: string): Promise<void> {
  await request('/api/push', { method: 'POST', body: JSON.stringify({ text }) })
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
