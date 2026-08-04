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
          // p-[2px] + thumb 18px + travel 18px = 40px track, keeps knob inside
          'relative inline-flex h-[22px] w-[40px] shrink-0 items-center overflow-hidden rounded-full p-[2px] transition-colors',
          checked ? 'bg-primary' : dark ? 'bg-[#374151]' : 'bg-[#CBD5E1]',
        )}
      >
        <span
          className={cn(
            'block size-[18px] shrink-0 rounded-full bg-white shadow-sm transition-transform duration-200 ease-out will-change-transform',
            checked ? 'translate-x-[18px]' : 'translate-x-0',
          )}
        />
      </button>
    </label>
  )
}
