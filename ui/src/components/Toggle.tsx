import { cn } from '@/lib/cn'

export function Toggle({
  checked,
  onChange,
  label,
  dark,
}: {
  checked: boolean
  onChange?: (v: boolean) => void
  label: string
  dark?: boolean
}) {
  return (
    <label className="flex cursor-pointer items-center justify-between gap-3">
      <span className={cn('text-[13px]', dark ? 'text-[#E8EDF5]' : 'text-[#1A2030]')}>{label}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange?.(!checked)}
        className={cn(
          'relative h-[22px] w-[40px] shrink-0 rounded-full transition-colors',
          checked ? 'bg-primary' : dark ? 'bg-[#374151]' : 'bg-[#CBD5E1]',
        )}
      >
        <span
          className={cn(
            'absolute top-[2px] size-[18px] rounded-full bg-white shadow transition-transform',
            checked ? 'translate-x-[20px]' : 'translate-x-[2px]',
          )}
        />
      </button>
    </label>
  )
}
