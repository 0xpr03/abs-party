// Remembers the host's last room settings so new rooms start with the same choices.

const STORAGE_KEY = 'absPartyRoomSettings'

export const DEFAULT_ROOM_SETTINGS = Object.freeze({
  host_only_controls: false,
  pause_on_join_leave: true,
})

export function loadRoomSettings() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY))
    return normalizeRoomSettings(saved)
  } catch {
    return { ...DEFAULT_ROOM_SETTINGS }
  }
}

export function saveRoomSettings(settings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(normalizeRoomSettings(settings)))
  } catch {
    // Storage unavailable (private mode etc.) — settings just won't be remembered
  }
}

export function normalizeRoomSettings(settings) {
  const out = { ...DEFAULT_ROOM_SETTINGS }
  for (const key of Object.keys(DEFAULT_ROOM_SETTINGS)) {
    if (typeof settings?.[key] === 'boolean') out[key] = settings[key]
  }
  return out
}
