import { useSyncExternalStore } from 'react'

// One shared 1 s clock for everything that shows ages or moving positions.
let now = Date.now()
const listeners = new Set<() => void>()
let timer: ReturnType<typeof setInterval> | undefined

function subscribe(listener: () => void) {
  listeners.add(listener)
  if (!timer) {
    now = Date.now()
    timer = setInterval(() => {
      now = Date.now()
      for (const l of listeners) l()
    }, 1000)
  }
  return () => {
    listeners.delete(listener)
    if (!listeners.size) {
      clearInterval(timer)
      timer = undefined
    }
  }
}

/** Current time in milliseconds, updated every second. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, () => now)
}
