import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { loadRoomSettings, saveRoomSettings, normalizeRoomSettings, DEFAULT_ROOM_SETTINGS } from './roomSettings.js'

describe('roomSettings', () => {
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

  it('returns defaults when nothing is stored', () => {
    expect(loadRoomSettings()).toEqual(DEFAULT_ROOM_SETTINGS)
  })

  it('recalls the last saved settings', () => {
    saveRoomSettings({ host_only_controls: true, pause_on_join_leave: false })
    expect(loadRoomSettings()).toEqual({ host_only_controls: true, pause_on_join_leave: false })
  })

  it('later saves overwrite earlier ones', () => {
    saveRoomSettings({ host_only_controls: true, pause_on_join_leave: false })
    saveRoomSettings({ host_only_controls: false, pause_on_join_leave: false })
    expect(loadRoomSettings()).toEqual({ host_only_controls: false, pause_on_join_leave: false })
  })

  it('falls back to defaults on corrupt storage', () => {
    localStorage.setItem('absPartyRoomSettings', '{not json')
    expect(loadRoomSettings()).toEqual(DEFAULT_ROOM_SETTINGS)
  })

  it('fills missing keys with defaults and drops unknown or mistyped ones', () => {
    expect(normalizeRoomSettings({ host_only_controls: true, pause_on_join_leave: 'no', extra: 1 }))
      .toEqual({ host_only_controls: true, pause_on_join_leave: true })
  })

  it('returns a fresh object so callers cannot mutate the defaults', () => {
    const s = loadRoomSettings()
    s.host_only_controls = true
    expect(DEFAULT_ROOM_SETTINGS.host_only_controls).toBe(false)
  })
})
