import fs from 'node:fs'
import path from 'node:path'

const {
  VERSION,
  UPDATE_ARCHIVE_PATH,
  UPDATE_ARCHIVE_URL,
  UPDATE_SIGNATURE_PATH,
  UPDATE_MANIFEST_PATH,
  RELEASE_NOTES,
} = process.env

const required = {
  VERSION,
  UPDATE_ARCHIVE_PATH,
  UPDATE_ARCHIVE_URL,
  UPDATE_SIGNATURE_PATH,
  UPDATE_MANIFEST_PATH,
}

const missing = Object.entries(required)
  .filter(([, value]) => !value)
  .map(([name]) => name)

if (missing.length > 0) {
  console.error(`[updater-manifest] missing env: ${missing.join(', ')}`)
  process.exit(1)
}

if (!fs.existsSync(UPDATE_ARCHIVE_PATH)) {
  console.error(`[updater-manifest] archive not found: ${UPDATE_ARCHIVE_PATH}`)
  process.exit(1)
}

if (!fs.existsSync(UPDATE_SIGNATURE_PATH)) {
  console.error(`[updater-manifest] signature not found: ${UPDATE_SIGNATURE_PATH}`)
  process.exit(1)
}

const signature = fs.readFileSync(UPDATE_SIGNATURE_PATH, 'utf8').trim()
const pubDate = new Date().toISOString()

const manifest = {
  version: VERSION,
  notes: RELEASE_NOTES || `AgentBro ${VERSION}`,
  pub_date: pubDate,
  platforms: {
    'darwin-aarch64': {
      signature,
      url: UPDATE_ARCHIVE_URL,
    },
    'darwin-x86_64': {
      signature,
      url: UPDATE_ARCHIVE_URL,
    },
  },
}

fs.mkdirSync(path.dirname(UPDATE_MANIFEST_PATH), { recursive: true })
fs.writeFileSync(UPDATE_MANIFEST_PATH, `${JSON.stringify(manifest, null, 2)}\n`)

console.log(`[updater-manifest] wrote ${UPDATE_MANIFEST_PATH}`)
