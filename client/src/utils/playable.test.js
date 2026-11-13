import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { whenPlayable } from './playable.js'

function fakeMedia(readyState = 0) {
  const media = new EventTarget()
  media.readyState = readyState
  media.seeking = false
  return media
}

describe('whenPlayable', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('fires once enough data is buffered', () => {
    const media = fakeMedia(1)
    const cb = vi.fn()
    whenPlayable(media, cb)
    vi.advanceTimersByTime(0)
    expect(cb).not.toHaveBeenCalled()

    media.readyState = 4
    media.dispatchEvent(new Event('canplaythrough'))
    expect(cb).toHaveBeenCalledTimes(1)
  })

  it('fires immediately when already buffered', () => {
    const cb = vi.fn()
    whenPlayable(fakeMedia(4), cb)
    vi.advanceTimersByTime(0)
    expect(cb).toHaveBeenCalledTimes(1)
  })

  it('ignores a stale ready event while still seeking', () => {
    const media = fakeMedia(4)
    media.seeking = true
    const cb = vi.fn()
    whenPlayable(media, cb)
    media.dispatchEvent(new Event('canplaythrough'))
    vi.advanceTimersByTime(5000)
    expect(cb).not.toHaveBeenCalled()

    media.seeking = false
    media.dispatchEvent(new Event('seeked'))
    expect(cb).toHaveBeenCalledTimes(1)
  })

  it('accepts HAVE_FUTURE_DATA only after the grace period', () => {
    const media = fakeMedia(3)
    const cb = vi.fn()
    whenPlayable(media, cb, { graceMs: 1500 })
    vi.advanceTimersByTime(1000)
    media.dispatchEvent(new Event('canplay'))
    expect(cb).not.toHaveBeenCalled()
    vi.advanceTimersByTime(500)
    expect(cb).toHaveBeenCalledTimes(1)
  })

  it('does nothing after cancel', () => {
    const media = fakeMedia(1)
    const cb = vi.fn()
    const cancel = whenPlayable(media, cb)
    cancel()
    media.readyState = 4
    media.dispatchEvent(new Event('canplaythrough'))
    vi.advanceTimersByTime(5000)
    expect(cb).not.toHaveBeenCalled()
  })

  it('fires only once', () => {
    const media = fakeMedia(4)
    const cb = vi.fn()
    whenPlayable(media, cb)
    vi.advanceTimersByTime(0)
    media.dispatchEvent(new Event('canplaythrough'))
    expect(cb).toHaveBeenCalledTimes(1)
  })
})
