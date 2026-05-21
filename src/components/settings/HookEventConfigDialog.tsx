import { useMemo, useState } from 'react'
import type { HookEventStatus, HookStatus } from '../../services/tauriApi'
import { PlatformIcon } from '../platform/PlatformIcon'

type HookCategory = HookEventStatus['category']
type HookCategoryState = 'on' | 'off' | 'mixed'

const hookCategoryOrder: HookCategory[] = ['approvals', 'notifications', 'lifecycle', 'activity']

function hookCategoryState(events: HookEventStatus[], enabled: Set<string>): HookCategoryState {
  const enabledCount = events.filter((event) => enabled.has(event.name)).length
  if (enabledCount === 0) return 'off'
  if (enabledCount === events.length) return 'on'
  return 'mixed'
}

function defaultHookEventSelection(hook: HookStatus) {
  const events = hook.events ?? []
  const supportedEventNames = new Set(events.map((event) => event.name))
  const enabledFromEvents = events.filter((event) => event.enabled).map((event) => event.name)
  if (enabledFromEvents.length > 0) return enabledFromEvents
  const enabledEventNames = (hook.enabledEventNames ?? []).filter((eventName) => supportedEventNames.has(eventName))
  if (enabledEventNames.length > 0) return enabledEventNames
  return events.map((event) => event.name)
}

export function HookEventConfigDialog({
  hook,
  busy,
  onClose,
  onSave,
}: {
  hook: HookStatus
  busy: boolean
  onClose: () => void
  onSave: (enabledEvents: string[]) => void
}) {
  const events = useMemo(() => {
    return (hook.events ?? [])
      .map((event, index) => ({ event, index }))
      .sort((left, right) => {
        const categoryDelta = hookCategoryOrder.indexOf(left.event.category) - hookCategoryOrder.indexOf(right.event.category)
        return categoryDelta || left.index - right.index
      })
      .map(({ event }) => event)
  }, [hook.events])
  const [enabled, setEnabled] = useState<Set<string>>(() => new Set(defaultHookEventSelection(hook)))

  const groups = useMemo(() => {
    return hookCategoryOrder
      .map((category) => {
        const categoryEvents = events.filter((event) => event.category === category)
        return {
          category,
          events: categoryEvents,
          title: categoryEvents[0]?.categoryTitle ?? category,
          subtitle: categoryEvents[0]?.categorySubtitle ?? '',
        }
      })
      .filter((group) => group.events.length > 0)
  }, [events])

  const selectedEvents = events.filter((event) => enabled.has(event.name)).map((event) => event.name)
  const canSave = selectedEvents.length > 0 && !busy

  const toggleCategory = (categoryEvents: HookEventStatus[]) => {
    const state = hookCategoryState(categoryEvents, enabled)
    setEnabled((current) => {
      const next = new Set(current)
      for (const event of categoryEvents) {
        if (state === 'on') next.delete(event.name)
        else next.add(event.name)
      }
      return next
    })
  }

  const toggleEvent = (eventName: string) => {
    setEnabled((current) => {
      const next = new Set(current)
      if (next.has(eventName)) next.delete(eventName)
      else next.add(eventName)
      return next
    })
  }

  return (
    <div
      className="hook-options-overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <section className="hook-options-dialog" role="dialog" aria-modal="true" aria-labelledby="hook-options-title">
        <div className="hook-options-header">
          <div className="hook-options-avatar">
            <PlatformIcon agentId={hook.adapterId || hook.name} displayName={hook.displayName || hook.name} size={34} />
          </div>
          <div>
            <h3 id="hook-options-title">{hook.displayName || hook.name}</h3>
            <p>调整已安装的 Hook 事件，保存后会刷新该客户端的 hooks 配置。</p>
          </div>
        </div>

        <div className="hook-options-scroll">
          <div className="hook-options-category-list">
            {groups.map((group) => {
              const state = hookCategoryState(group.events, enabled)
              return (
                <button
                  key={group.category}
                  type="button"
                  className="hook-options-category-row"
                  onClick={() => toggleCategory(group.events)}
                >
                  <span className="hook-options-category-icon">{group.title.slice(0, 1)}</span>
                  <span className="hook-options-category-copy">
                    <strong>{group.title}</strong>
                    <span>{group.subtitle}</span>
                  </span>
                  <span className={`hook-options-indicator hook-options-indicator--${state}`} aria-hidden="true">
                    {state === 'on' ? '✓' : state === 'mixed' ? '-' : ''}
                  </span>
                </button>
              )
            })}
          </div>

          <details className="hook-options-advanced" open>
            <summary>高级：按事件单独配置</summary>
            <div className="hook-options-event-groups">
              {groups.map((group) => (
                <div key={group.category} className="hook-options-event-group">
                  <div className="hook-options-event-group-title">{group.title}</div>
                  {group.events.map((event) => {
                    const checked = enabled.has(event.name)
                    return (
                      <button
                        key={event.name}
                        type="button"
                        className="hook-options-event-row"
                        onClick={() => toggleEvent(event.name)}
                      >
                        <span className="hook-options-event-row-toggle">
                          <span className="hook-options-event-copy">
                            <strong>{event.name}</strong>
                          </span>
                          <span className={`hook-options-indicator hook-options-indicator--${checked ? 'on' : 'off'}`} aria-hidden="true">
                            {checked ? '✓' : ''}
                          </span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              ))}
            </div>
          </details>
        </div>

        <div className="hook-options-footer">
          <span className={selectedEvents.length === 0 ? 'hook-options-warning' : ''}>
            {selectedEvents.length === 0
              ? '至少需要启用一个事件。'
              : `已启用 ${selectedEvents.length}/${events.length} 个事件；关闭后对应通知或审批将不再触发。`}
          </span>
          <div>
            <button type="button" className="hook-options-cancel" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button type="button" className="hook-options-save" onClick={() => onSave(selectedEvents)} disabled={!canSave}>
              {busy ? '保存中...' : '保存'}
            </button>
          </div>
        </div>
      </section>
    </div>
  )
}
