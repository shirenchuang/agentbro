import { randomUUID } from 'node:crypto'
import { access, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

export const inject = ['sessions', 'approval']

export function apply(ctx) {
  const output = process.env.AGENTBRO_HARNESS_FIXTURE_OUTPUT
  if (!output) return
  const timer = setTimeout(() => {
    run(ctx, output).catch(error => writeFile(join(output, 'error.txt'), String(error.stack ?? error)))
  }, 2000)
  ctx.effect(() => () => clearTimeout(timer))
}

async function run(ctx, output) {
  const session = ctx.sessions.create()
  session.append('turn/start', { turn: 1 })
  session.append('user/message', {
    id: randomUUID(),
    role: 'user',
    source: { kind: 'user' },
    content: [{ type: 'text', text: 'AgentBro Harness fixture' }],
  }, { surfaceOp: 'append' })
  await writeFile(join(output, 'session.txt'), session.id)
  for (const [index, toolName] of ['Read', 'Read'].entries()) {
    await waitFor(join(output, `request-${index}`))
    const decision = await ctx.approval.request({ agent: { session }, toolName, reason: `Read the test fixture ${index + 1}` })
    await writeFile(join(output, `decision-${index}.txt`), decision)
  }
  session.append('turn/end', { turn: 1, reason: { kind: 'completed' } })
}

async function waitFor(path) {
  for (let attempt = 0; attempt < 240; attempt++) {
    if (await access(path).then(() => true, () => false)) return
    await delay(250)
  }
  throw new Error(`Timed out waiting for ${path}`)
}
