/** Coalesce updates into one frame. An unchanged map schedules no work. */
export function createRenderScheduler(render: () => void, request = requestAnimationFrame, cancel = cancelAnimationFrame) {
  let frame: number | null = null
  let running = false
  const invalidate = () => {
    if (!running || frame !== null) return
    frame = request(() => { frame = null; render() })
  }
  return {
    invalidate,
    start() { running = true; invalidate() },
    stop() { running = false; if (frame !== null) cancel(frame); frame = null },
  }
}

/** Reuse GPU layer descriptors until their actual data changes. */
export function memoizeLayer<T, R>(build: (data: T) => R): (data: T) => R {
  let previous: T
  let result: R
  let initialized = false
  return (data) => {
    if (!initialized || data !== previous) {
      result = build(data)
      previous = data
      initialized = true
    }
    return result
  }
}
