import { describe, expect, it } from 'vitest'
import { getWritePermissionPreview, shortenPath } from '../utils/permissionPreview'

describe('permissionPreview path formatting', () => {
  it('shortens Unix paths with Unix separators', () => {
    expect(shortenPath('/Users/me/project/src/components/App.tsx')).toBe('.../src/components/App.tsx')
  })

  it('shortens Windows paths with Windows separators', () => {
    expect(shortenPath('C:\\Users\\me\\project\\src\\components\\App.tsx')).toBe('...\\src\\components\\App.tsx')
  })

  it('uses Windows path previews for write permission payloads', () => {
    const preview = getWritePermissionPreview({
      file_path: 'C:\\Users\\me\\project\\src\\main.rs',
      content: 'one\ntwo\nthree',
    })

    expect(preview.shortPath).toBe('...\\project\\src\\main.rs')
    expect(preview.visibleContent).toContain('one')
  })
})
