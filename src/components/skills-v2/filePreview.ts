export function isMarkdownPath(path: string | null): boolean {
  return Boolean(path && /\.(md|mdx|markdown)$/i.test(path))
}
