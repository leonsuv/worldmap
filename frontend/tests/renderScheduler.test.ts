import { describe, it, expect, vi } from 'vitest'
import { createRenderScheduler, memoizeLayer } from '../src/layers/renderScheduler'

describe('event-driven rendering', () => {
  it('coalesces bursts and schedules no frames while idle', () => {
    let callback: FrameRequestCallback | undefined
    const request = vi.fn((fn: FrameRequestCallback) => { callback = fn; return 1 })
    const render = vi.fn()
    const scheduler = createRenderScheduler(render, request, vi.fn())
    scheduler.start()
    for (let i = 0; i < 10000; i++) scheduler.invalidate()
    expect(request).toHaveBeenCalledTimes(1)
    callback!(0)
    expect(render).toHaveBeenCalledTimes(1)
    expect(request).toHaveBeenCalledTimes(1)
    scheduler.invalidate()
    expect(request).toHaveBeenCalledTimes(2)
  })
  it('cancels pending frames on unmount and can restart', () => {
    const request = vi.fn(() => 7)
    const cancel = vi.fn()
    const scheduler = createRenderScheduler(vi.fn(), request, cancel)
    scheduler.start(); scheduler.stop(); scheduler.invalidate()
    expect(cancel).toHaveBeenCalledWith(7)
    expect(request).toHaveBeenCalledTimes(1)
    scheduler.start()
    expect(request).toHaveBeenCalledTimes(2)
  })
  it('reuses layer objects for unchanged datasets', () => {
    const build = vi.fn((data: number[]) => ({ data }))
    const memoized = memoizeLayer(build)
    const data = [1, 2]
    const first = memoized(data)
    for (let i = 0; i < 1000; i++) expect(memoized(data)).toBe(first)
    expect(build).toHaveBeenCalledTimes(1)
    expect(memoized([3])).not.toBe(first)
  })
})
