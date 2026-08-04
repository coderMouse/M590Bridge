import type { ButtonHTMLAttributes, ReactNode } from 'react'
import { cn } from '@/lib/cn'
import { Loader2 } from 'lucide-react'

export function PrimaryButton({
  children,
  loading,
  variant = 'primary',
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode
  loading?: boolean
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
}) {
  return (
    <button
      type="button"
      className={cn(
        'inline-flex h-10 w-full items-center justify-center gap-2 rounded-md px-3 text-[13px] font-semibold transition-colors disabled:opacity-60',
        variant === 'primary' && 'bg-primary text-white hover:bg-[#1D4ED8]',
        variant === 'secondary' && 'border border-black/10 bg-white text-[#1A2030] hover:bg-[#F8FAFC]',
        variant === 'ghost' && 'bg-transparent text-primary hover:bg-[#EFF6FF]',
        variant === 'danger' && 'bg-destructive text-white hover:bg-red-700',
        className,
      )}
      disabled={loading || props.disabled}
      {...props}
    >
      {loading ? <Loader2 size={14} className="animate-spin" /> : null}
      {children}
    </button>
  )
}
