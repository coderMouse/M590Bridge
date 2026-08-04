/** Brand tokens from Figma Make M590Bridge-UI-Kit-Design */
export const C = {
  blue: '#2563EB',
  blueHover: '#1D4ED8',
  blueLight: '#DBEAFE',
  blueFaint: '#EFF6FF',
  green: '#16A34A',
  greenLight: '#DCFCE7',
  amber: '#D97706',
  amberLight: '#FEF3C7',
  red: '#DC2626',
  redLight: '#FEE2E2',
  bg: '#F5F7FA',
  card: '#FFFFFF',
  surface: '#F1F4F7',
  border: 'rgba(0,0,0,0.08)',
  borderMed: 'rgba(0,0,0,0.13)',
  text: '#1A2030',
  textMuted: '#6B7589',
  textLight: '#9AA3B2',
  darkBg: '#0F1117',
  darkCard: '#1C2030',
  darkSurface: '#252D3D',
  darkBorder: 'rgba(255,255,255,0.08)',
  darkBorderMed: 'rgba(255,255,255,0.13)',
  darkText: '#E8EDF5',
  darkTextMuted: '#8896AC',
  darkTextLight: '#5A6680',
} as const

export type ConnectionStatus =
  | '已连接'
  | '同步中'
  | '已暂停'
  | '连接中'
  | '未连接'
  | '出错'

export type ThemeMode = 'light' | 'dark'
