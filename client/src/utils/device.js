// Device-level preferences for the room page (not room settings: these never leave the browser).

const KEEP_AWAKE_KEY = 'absPartyKeepAwake'
const MOBILE_HINT_KEY = 'absPartyMobileHintDismissed'

// Touch-only device: phones and tablets, whose OS suspends pages that aren't on screen
export function isMobileDevice() {
  return typeof matchMedia === 'function' && matchMedia('(pointer: coarse) and (hover: none)').matches
}

// The user's own choice wins; until they make one, keep the screen on only on mobile
export function loadKeepAwake(isMobile) {
  const saved = read(KEEP_AWAKE_KEY)
  return saved === null ? isMobile : saved === 'true'
}

export function saveKeepAwake(on) {
  write(KEEP_AWAKE_KEY, String(Boolean(on)))
}

export function mobileHintDismissed() {
  return read(MOBILE_HINT_KEY) === 'true'
}

export function dismissMobileHint() {
  write(MOBILE_HINT_KEY, 'true')
}

function read(key) {
  try { return localStorage.getItem(key) } catch { return null }
}

function write(key, value) {
  try { localStorage.setItem(key, value) } catch {
    // Storage unavailable (private mode etc.) — the choice just won't be remembered
  }
}
