import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { isMobileDevice, loadKeepAwake, saveKeepAwake, mobileHintDismissed, dismissMobileHint } from './device.js'

describe('device preferences', () => {
  beforeEach(() => {
    const store = new Map()
    vi.stubGlobal('localStorage', {
      getItem: (k) => (store.has(k) ? store.get(k) : null),
      setItem: (k, v) => store.set(k, String(v)),
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('keeps the screen on by default only on mobile', () => {
    expect(loadKeepAwake(true)).toBe(true)
    expect(loadKeepAwake(false)).toBe(false)
  })

  it('remembers an explicit choice on any device', () => {
    saveKeepAwake(false)
    expect(loadKeepAwake(true)).toBe(false)
    saveKeepAwake(true)
    expect(loadKeepAwake(false)).toBe(true)
  })

  it('remembers that the mobile hint was dismissed', () => {
    expect(mobileHintDismissed()).toBe(false)
    dismissMobileHint()
    expect(mobileHintDismissed()).toBe(true)
  })

  it('falls back to defaults when storage throws', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => { throw new Error('denied') },
      setItem: () => { throw new Error('denied') },
    })
    expect(loadKeepAwake(true)).toBe(true)
    expect(() => saveKeepAwake(false)).not.toThrow()
    expect(mobileHintDismissed()).toBe(false)
  })

  it('detects touch-only devices', () => {
    vi.stubGlobal('matchMedia', (q) => ({ matches: q === '(pointer: coarse) and (hover: none)' }))
    expect(isMobileDevice()).toBe(true)
    vi.stubGlobal('matchMedia', () => ({ matches: false }))
    expect(isMobileDevice()).toBe(false)
  })
})
