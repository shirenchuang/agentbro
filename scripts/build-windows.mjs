import { spawnSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const rootDir = resolve(scriptDir, '..')
const config = JSON.stringify({ bundle: { createUpdaterArtifacts: false } })

const result = spawnSync(
  'cargo',
  ['tauri', 'build', '--bundles', 'nsis,msi', '--ci', '--config', config],
  {
    cwd: rootDir,
    stdio: 'inherit',
  },
)

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}
