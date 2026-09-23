/** Coalesce redraw requests into one frame; an unchanged map schedules no work. */
export function createRenderScheduler(render: () => void, request = requestAnimationFrame, cancel = cancelAnimationFrame) {
  let frame: number | null = null
  let running = false
  const invalidate = () => {
    if (!running || frame !== null) return
    frame = request(() => {
      frame = null
      render()
    })
  }
  return {
    invalidate,
    start() {
      running = true
      invalidate()
    },
    stop() {
      running = false
      if (frame !== null) cancel(frame)
      frame = null
    },
  }
}

/** Reuse a built value until one of its inputs changes (compared by identity). */
export function memo<A extends unknown[], R>(build: (...args: A) => R): (...args: A) => R {
  let previous: A | null = null
  let result: R
  return (...args: A) => {
    if (!previous || args.length !== previous.length || args.some((a, i) => a !== previous![i])) {
      result = build(...args)
      previous = args
    }
    return result
  }
}

// The deck.gl render loop registers itself here so data modules can request
// a redraw without importing the map code.
let redraw: () => void = () => {}
export function setRedraw(fn: () => void) {
  redraw = fn
}
export function requestRedraw() {
  redraw()
}

/** Tiny observable used by data feeds so React panels can follow live data. */
export function createSignal() {
  const listeners = new Set<() => void>()
  let version = 0
  return {
    subscribe(fn: () => void) {
      listeners.add(fn)
      return () => {
        listeners.delete(fn)
      }
    },
    version: () => version,
    emit() {
      version++
      for (const fn of listeners) fn()
    },
  }
}
