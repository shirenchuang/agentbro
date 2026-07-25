import type { AnchorHTMLAttributes, MouseEvent } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { open as openShell } from '@tauri-apps/plugin-shell'
import { isTauri } from '../../services/tauriApi'

const components = {
  a: MarkdownLink,
}

export function MarkdownContent({ content }: { content: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
      {content}
    </ReactMarkdown>
  )
}

function MarkdownLink({ href, children, onClick, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event)
    if (event.defaultPrevented || !href || href.startsWith('#')) return
    event.preventDefault()
    openMarkdownHref(href)
  }

  return (
    <a {...props} href={href} target="_blank" rel="noreferrer" onClick={handleClick}>
      {children}
    </a>
  )
}

function openMarkdownHref(href: string) {
  const target = href.trim()
  if (!target || /^javascript:/i.test(target)) return
  if (isTauri()) {
    openShell(target).catch((err) => console.warn('[skills] open markdown link:', err))
  } else {
    window.open(target, '_blank', 'noopener,noreferrer')
  }
}
