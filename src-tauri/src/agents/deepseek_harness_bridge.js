import { createConnection } from 'node:net'
import { randomUUID } from 'node:crypto'

const agent = 'deepseek-harness'
const socketPath = process.env.AGENTBRO_HOOK_SOCKET ?? '/tmp/agentbro-' + process.getuid?.() + '.sock'

function send(payload, timeout, signal) {
  if (signal?.aborted || !process.getuid) return Promise.resolve(null)
  return new Promise((resolve) => {
    const socket = createConnection(socketPath)
    let response = ''
    const finish = (value) => {
      signal?.removeEventListener('abort', abort)
      socket.destroy()
      resolve(value)
    }
    const abort = () => finish(null)
    signal?.addEventListener('abort', abort, { once: true })
    if (signal?.aborted) return abort()
    socket.setEncoding('utf8')
    socket.setTimeout(timeout, abort)
    socket.on('error', abort)
    socket.on('end', abort)
    socket.on('connect', () => socket.write(JSON.stringify(payload) + '\n'))
    socket.on('data', (data) => {
      response += data
      if (response.length > 8192) return abort()
      const end = response.indexOf('\n')
      if (end === -1) return
      try {
        finish(JSON.parse(response.slice(0, end)))
      } catch {
        abort()
      }
    })
  })
}

function envelope(session, event) {
  return {
    agent,
    event,
    session_id: agent + ':' + session.id,
    cwd: session.header?.cwd ?? '',
    _term_bundle_id: 'com.deepseek.dsh',
  }
}

export function apply(ctx) {
  const lifetime = new AbortController()
  ctx.effect(() => () => lifetime.abort())
  const updates = new Map()
  const pending = new Set()

  ctx.on('session/event', (session, event) => {
    if (event.type !== 'turn/start' && event.type !== 'turn/end') return
    const type = event.type === 'turn/start' ? 'processing' : 'turn_end'
    const previous = updates.get(session.id) ?? Promise.resolve()
    const sent = previous.then(() => send({
      ...envelope(session, type),
      outcome: type === 'turn_end' ? event.data?.reason?.kind : undefined,
    }, 1500, lifetime.signal))
    updates.set(session.id, sent)
    void sent.finally(() => {
      if (updates.get(session.id) === sent) updates.delete(session.id)
    })
  })

  ctx.on('approval/request', async (request, next) => {
    const session = request.agent.session
    const reason = request.displayReason?.zh ?? request.reason ?? request.displayReason?.en
    if (!reason?.trim() || pending.has(session.id)) return next()
    pending.add(session.id)
    const signal = request.signal
      ? AbortSignal.any([request.signal, lifetime.signal])
      : lifetime.signal
    try {
      await updates.get(session.id)
      const reply = await send({
        ...envelope(session, 'permission_request'),
        tool_use_id: randomUUID(),
        tool_name: request.toolName,
        tool_input: { description: reason },
      }, 300000, signal)
      if (request.signal?.aborted) return 'cancelled'
      if (reply?.decision === 'allow' && reply.always !== true) return 'allowed-once'
      if (reply?.decision === 'deny') return 'rejected'
      return next()
    } finally {
      pending.delete(session.id)
    }
  }, { prepend: true })
}
