import type { MouseEvent } from 'react'

export function runIslandAction(event: MouseEvent<HTMLButtonElement>, action: () => void) {
  if (event.button !== 0 || (event.type === 'click' && event.detail !== 0)) return
  event.preventDefault()
  event.stopPropagation()
  action()
}

export function islandActionProps(action: () => void) {
  const handleAction = (event: MouseEvent<HTMLButtonElement>) => runIslandAction(event, action)
  return {
    // Handle the pointer action before the Island can lose focus.
    onMouseDown: handleAction,
    onClick: handleAction,
  }
}
