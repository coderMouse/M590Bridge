import { C } from '@/lib/tokens'

export function AppIcon({ size = 32, dark = false }: { size?: number; dark?: boolean }) {
  const cardBg = dark ? C.darkCard : '#FFFFFF'
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden>
      <rect x="1.5" y="7" width="10" height="13" rx="1.5" stroke={C.blue} strokeWidth="1.5" fill={cardBg} />
      <rect x="3" y="8.5" width="7" height="8" rx="0.5" fill={C.blueLight} opacity="0.7" />
      <rect x="20.5" y="7" width="10" height="13" rx="1.5" stroke={C.blue} strokeWidth="1.5" fill={cardBg} />
      <rect x="22" y="8.5" width="7" height="8" rx="0.5" fill={C.blueLight} opacity="0.7" />
      <line x1="11.5" y1="13.5" x2="13" y2="13.5" stroke={C.blue} strokeWidth="1.4" strokeDasharray="1.5 1.5" strokeLinecap="round" />
      <line x1="19" y1="13.5" x2="20.5" y2="13.5" stroke={C.blue} strokeWidth="1.4" strokeDasharray="1.5 1.5" strokeLinecap="round" />
      <rect x="13" y="10" width="6" height="8" rx="1" stroke={C.blue} strokeWidth="1.4" fill={cardBg} />
      <rect x="14.5" y="8.5" width="3" height="3" rx="0.6" stroke={C.blue} strokeWidth="1.2" fill={cardBg} />
      <line x1="14.5" y1="13" x2="17.5" y2="13" stroke={C.blue} strokeWidth="1" strokeLinecap="round" />
      <line x1="14.5" y1="15" x2="17.5" y2="15" stroke={C.blue} strokeWidth="1" strokeLinecap="round" />
    </svg>
  )
}
