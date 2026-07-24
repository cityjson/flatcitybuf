// src/components/ui.tsx
// Small themed primitives shared by the sidebar panels, so the CityJSON look
// (gold section accents, purple primary buttons) stays consistent in one place.
import type { ButtonHTMLAttributes, ReactNode } from 'react'

/** A section heading with the CityJSON gold accent bar. */
export function SectionHeading({ children }: { children: ReactNode }) {
  return (
    <h2 className="flex items-center gap-2 text-sm font-semibold text-cj-charcoal">
      <span className="h-3.5 w-1 rounded-sm bg-cj-gold" aria-hidden />
      {children}
    </h2>
  )
}

/** The primary (purple) call-to-action button. */
export function PrimaryButton(
  { className = '', ...props }: ButtonHTMLAttributes<HTMLButtonElement>,
) {
  return (
    <button
      className={
        'rounded bg-cj-purple px-3 py-1 text-sm font-medium text-white '
        + 'transition-colors hover:bg-cj-purple-dark '
        + 'disabled:cursor-not-allowed disabled:opacity-50 '
        + className
      }
      {...props}
    />
  )
}
