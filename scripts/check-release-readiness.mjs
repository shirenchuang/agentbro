import fs from 'node:fs'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
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

function countPresentEnv(names) {
  return names.filter((name) => process.env[name]?.trim()).length
}

function gitOutput(args) {
  try {
    return execFileSync('git', args, {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return ''
  }
}

function stableVersion(value) {
  const match = value.match(/^v?(\d+)\.(\d+)\.(\d+)$/)
  return match ? match.slice(1).map(Number) : null
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index]
  }
  return 0
}

function latestStableTag() {
  const override = process.env.RELEASE_CHECK_LATEST_STABLE_TAG?.trim()
  if (override) return override
  return gitOutput(['tag', '--list', 'v*', '--sort=-version:refname'])
    .split(/\r?\n/)
    .find((tag) => stableVersion(tag)) || ''
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
requireEqual('package.json license', pkg.license, 'Apache-2.0')
requireEqual('Cargo license', packageTomlField(cargoToml, 'license'), 'Apache-2.0')
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
} else {
  const branchName = process.env.GITHUB_BASE_REF || refName || gitOutput(['branch', '--show-current'])
  const protectedDevelopmentBranches = new Set(['dev', 'develop', 'main', 'master'])
  if (protectedDevelopmentBranches.has(branchName)) {
    const latestTag = latestStableTag()
    const current = stableVersion(packageVersion)
    const latest = stableVersion(latestTag)
    if (latest && current && compareVersions(current, latest) < 0) {
      errors.push(
        `package version ${packageVersion} is behind latest stable tag ${latestTag}; merge main back into dev after each release`,
      )
    } else if (!latestTag) {
      warnings.push('No stable Git tag was found; branch version freshness could not be checked.')
    }
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

for (const relativePath of releaseFiles) {
  const absolutePath = path.join(root, relativePath)
  if (!fs.existsSync(absolutePath)) {
    errors.push(`${relativePath} is missing`)
  }
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
    if (!process.env.HOMEBREW_TAP_TOKEN?.trim()) {
      warnings.push('HOMEBREW_TAP_TOKEN is not set; stable DMG release will proceed without updating Homebrew.')
    }

    if (!process.env.OSS_ACCESS_KEY_ID?.trim() || !process.env.OSS_ACCESS_KEY_SECRET?.trim()) {
      warnings.push('OSS_ACCESS_KEY_ID/OSS_ACCESS_KEY_SECRET not set; stable release will skip the Aliyun OSS mirror and the Homebrew cask will point at GitHub.')
    }

    const telemetryEnvNames = [
      'AGENTBRO_TELEMETRY_SLS_HOST',
      'AGENTBRO_TELEMETRY_SLS_PROJECT',
      'AGENTBRO_TELEMETRY_SLS_LOGSTORE',
    ]
    const telemetryEnvCount = countPresentEnv(telemetryEnvNames)
    if (telemetryEnvCount > 0 && telemetryEnvCount < telemetryEnvNames.length) {
      errors.push('AGENTBRO_TELEMETRY_SLS_HOST, AGENTBRO_TELEMETRY_SLS_PROJECT, and AGENTBRO_TELEMETRY_SLS_LOGSTORE must all be set together')
    } else if (telemetryEnvCount === 0) {
      warnings.push('AgentBro anonymous telemetry SLS target is not set; release builds will not upload anonymous usage stats.')
    }
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
