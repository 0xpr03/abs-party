import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { effectScope, ref, nextTick } from 'vue'
import { useWakeLock } from './useWakeLock.js'

function fakeLock() {
  const listeners = []
  return {
    released: false,
    addEventListener: (_, fn) => listeners.push(fn),
    release() { this.released = true; listeners.forEach(fn => fn()); return Promise.resolve() },
  }
}

const flush = () => new Promise(r => setTimeout(r, 0))

describe('useWakeLock', () => {
  let locks, visibility, scope

  beforeEach(() => {
    locks = []
    visibility = 'visible'
    vi.stubGlobal('navigator', { wakeLock: { request: vi.fn(async () => { const l = fakeLock(); locks.push(l); return l }) } })
    vi.spyOn(document, 'visibilityState', 'get').mockImplementation(() => visibility)
    scope = effectScope()
  })

  afterEach(() => {
    scope.stop()
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  function setVisibility(v) {
    visibility = v
    document.dispatchEvent(new Event('visibilitychange'))
  }

  it('holds a lock while enabled and releases it when switched off', async () => {
    const enabled = ref(true)
    const wl = scope.run(() => useWakeLock(enabled))
    await flush()
    expect(wl.active.value).toBe(true)
    enabled.value = false
    await nextTick()
    expect(locks[0].released).toBe(true)
    expect(wl.active.value).toBe(false)
  })

  it('requests nothing while disabled', async () => {
    scope.run(() => useWakeLock(ref(false)))
    await flush()
    expect(navigator.wakeLock.request).not.toHaveBeenCalled()
  })

  it('re-acquires after the browser drops the lock on hiding the page', async () => {
    const wl = scope.run(() => useWakeLock(ref(true)))
    await flush()
    setVisibility('hidden')
    locks[0].release() // what the browser does
    expect(wl.active.value).toBe(false)
    setVisibility('visible')
    await flush()
    expect(locks).toHaveLength(2)
    expect(wl.active.value).toBe(true)
  })

  it('releases the lock when the room is left', async () => {
    scope.run(() => useWakeLock(ref(true)))
    await flush()
    scope.stop()
    expect(locks[0].released).toBe(true)
  })

  it('stays inactive when the browser refuses', async () => {
    navigator.wakeLock.request.mockRejectedValueOnce(new Error('NotAllowedError'))
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const wl = scope.run(() => useWakeLock(ref(true)))
    await flush()
    expect(wl.active.value).toBe(false)
    await wl.acquire()
    expect(wl.active.value).toBe(true)
  })
})
