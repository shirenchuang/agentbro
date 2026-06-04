import { spawnSync } from 'node:child_process'
import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const rootDir = resolve(scriptDir, '..')
const tauriDir = join(rootDir, 'src-tauri')
const targetDir = join(tauriDir, 'target')
const resourceDir = join(targetDir, 'agentbro-bridge-resource')
const resourcePath = join(resourceDir, 'agentbro-bridge')
const release = process.argv.includes('--release')
const profile = release ? 'release' : 'debug'
const binaryName = process.platform === 'win32' ? 'agentbro-bridge.exe' : 'agentbro-bridge'
const binaryPath = join(targetDir, profile, binaryName)

mkdirSync(resourceDir, { recursive: true })
writeFileSync(resourcePath, '')

const args = ['build', '--manifest-path', join(tauriDir, 'Cargo.toml'), '--bin', 'agentbro-bridge']
if (release) {
  args.push('--release')
}

const result = spawnSync('cargo', args, {
  cwd: rootDir,
  stdio: 'inherit',
})

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}

copyFileSync(binaryPath, resourcePath)
