import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const errors = []
const warnings = []

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8')
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath))
}

function packageTomlField(content, field) {
  let inPackage = false

  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim()

    if (trimmed === '[package]') {
      inPackage = true
      continue
    }

    if (inPackage && trimmed.startsWith('[')) break

    if (inPackage) {
      const match = trimmed.match(new RegExp(`^${field}\\s*=\\s*"([^"]+)"`))
      if (match) return match[1]
    }
  }

  return null
}

function cargoLockPackageVersion(content, packageName) {
  let inPackage = false
  let currentName = null

  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim()

    if (trimmed === '[[package]]') {
      inPackage = true
      currentName = null
      continue
    }

    if (!inPackage) continue

    const nameMatch = trimmed.match(/^name\s*=\s*"([^"]+)"/)
    if (nameMatch) {
      currentName = nameMatch[1]
      continue
    }

    const versionMatch = trimmed.match(/^version\s*=\s*"([^"]+)"/)
    if (versionMatch && currentName === packageName) return versionMatch[1]
  }

  return null
}

function requireEqual(label, actual, expected) {
  if (actual !== expected) errors.push(`${label} is ${actual || '<missing>'}, expected ${expected}`)
}

function requireEnv(name) {
  if (!process.env[name]?.trim()) errors.push(`${name} is required for stable releases`)
}

const pkg = readJson('package.json')
const tauri = readJson('src-tauri/tauri.conf.json')
const cargoToml = read('src-tauri/Cargo.toml')
const cargoLock = read('src-tauri/Cargo.lock')

const packageVersion = pkg.version
const tauriVersion = tauri.version
const cargoName = packageTomlField(cargoToml, 'name')
const cargoVersion = packageTomlField(cargoToml, 'version')
const cargoDefaultRun = packageTomlField(cargoToml, 'default-run')
const cargoLockVersion = cargoLockPackageVersion(cargoLock, 'agentbro')

requireEqual('package.json name', pkg.name, 'agentbro')
requireEqual('Tauri productName', tauri.productName, 'AgentBro')
requireEqual('Tauri identifier', tauri.identifier, 'com.agentbro.desktop')
requireEqual('Tauri updater artifact generation', String(tauri.bundle?.createUpdaterArtifacts), 'true')
requireEqual('Cargo package name', cargoName, 'agentbro')
requireEqual('Cargo default-run', cargoDefaultRun, 'agentbro')
requireEqual('Tauri version', tauriVersion, packageVersion)
requireEqual('Cargo version', cargoVersion, packageVersion)
requireEqual('Cargo.lock agentbro version', cargoLockVersion, packageVersion)

const refName = process.env.GITHUB_REF_NAME || ''
const tagVersion = refName.startsWith('v') ? refName.slice(1) : ''
const isPrerelease = tagVersion.includes('-')
if (tagVersion) {
  if (isPrerelease) {
    requireEqual('Git tag base version', tagVersion.split('-')[0], packageVersion.split('-')[0])
  } else {
    requireEqual('Git tag version', tagVersion, packageVersion)
  }
}

const releaseFiles = [
  '.github/workflows/build.yml',
  '.github/workflows/release.yml',
  'build.sh',
  'homebrew/Casks/agentbro.rb',
  'package.json',
  'src-tauri/Cargo.toml',
  'src-tauri/tauri.conf.json',
]

const stalePatterns = ['Agent Island', 'AgentIsland', 'agent-island', 'com.agent-island.app']
for (const relativePath of releaseFiles) {
  const absolutePath = path.join(root, relativePath)
  if (!fs.existsSync(absolutePath)) {
    errors.push(`${relativePath} is missing`)
    continue
  }

  const content = fs.readFileSync(absolutePath, 'utf8')
  for (const pattern of stalePatterns) {
    if (content.includes(pattern)) {
      errors.push(`${relativePath} still contains stale release name: ${pattern}`)
    }
  }
}

if (fs.existsSync(path.join(root, 'homebrew/Casks/agent-island.rb'))) {
  errors.push('homebrew/Casks/agent-island.rb should be renamed to homebrew/Casks/agentbro.rb')
}

const updaterPubkey = tauri.plugins?.updater?.pubkey || ''
const strictRelease = process.env.CI_RELEASE === '1'
const allowUnsignedRelease = process.env.ALLOW_UNSIGNED_RELEASE === '1' || isPrerelease
if (!updaterPubkey.trim()) {
  const message = 'Tauri updater pubkey is empty; stable releases cannot ship signed auto-updates.'
  if (strictRelease) {
    errors.push(message)
  } else {
    warnings.push(message)
  }
}

if (strictRelease) {
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY?.trim() && !process.env.TAURI_SIGNING_PRIVATE_KEY_PATH?.trim()) {
    errors.push('TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH is required for updater signatures')
  }

  if (!allowUnsignedRelease) {
    requireEnv('CERTIFICATE_P12')
    requireEnv('CERTIFICATE_PASSWORD')
    requireEnv('CODESIGN_IDENTITY')
    requireEnv('APPLE_ID')
    requireEnv('APPLE_PASSWORD')
    requireEnv('APPLE_TEAM_ID')
    requireEnv('HOMEBREW_TAP_TOKEN')
  } else {
    warnings.push('unsigned prerelease mode enabled; Apple signing, notarization, and Homebrew update are skipped.')
  }
}

for (const warning of warnings) {
  console.warn(`[release:check] warning: ${warning}`)
}

if (errors.length > 0) {
  console.error('[release:check] failed:')
  for (const error of errors) console.error(`- ${error}`)
  process.exit(1)
}

console.log('[release:check] ok')
