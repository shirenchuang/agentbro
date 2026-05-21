import type { AnchorHTMLAttributes } from 'react'
import { open } from '@tauri-apps/plugin-shell'

export function ExternalLink({ href, children, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return (
    <a
      {...props}
      href={href}
      onClick={(e) => {
        e.preventDefault()
        e.stopPropagation()
        if (href) {
          open(href).catch((err) => console.warn('[ExternalLink] open:', err))
        }
      }}
    >
      {children}
    </a>
  )
}
