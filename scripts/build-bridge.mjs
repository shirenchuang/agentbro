import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
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

function cargoCommand() {
  if (process.env.CARGO) {
    return process.env.CARGO
  }

  if (process.platform === 'win32') {
    const candidates = [
      process.env.CARGO_HOME ? join(process.env.CARGO_HOME, 'bin', 'cargo.exe') : null,
      process.env.USERPROFILE ? join(process.env.USERPROFILE, '.cargo', 'bin', 'cargo.exe') : null,
    ].filter(Boolean)

    const cargo = candidates.find((candidate) => existsSync(candidate))
    if (cargo) {
      return cargo
    }
  }

  return 'cargo'
}

mkdirSync(resourceDir, { recursive: true })
writeFileSync(resourcePath, '')

const args = ['build', '--manifest-path', join(tauriDir, 'Cargo.toml'), '--bin', 'agentbro-bridge']
if (release) {
  args.push('--release')
}

const cargo = cargoCommand()
const result = spawnSync(cargo, args, {
  cwd: rootDir,
  stdio: 'inherit',
})

if (result.error) {
  console.error(`Failed to run ${cargo}: ${result.error.message}`)
}

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}

copyFileSync(binaryPath, resourcePath)
