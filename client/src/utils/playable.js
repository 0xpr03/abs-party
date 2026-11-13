// Calls onReady once the media element has data to play from its current position.
// Prefers HAVE_ENOUGH_DATA; accepts HAVE_FUTURE_DATA after graceMs, because browsers
// may never reach HAVE_ENOUGH_DATA while paused. Returns a cancel function.

const HAVE_FUTURE_DATA = 3
const HAVE_ENOUGH_DATA = 4
const EVENTS = ['canplay', 'canplaythrough', 'seeked', 'loadeddata', 'progress']

export function whenPlayable(media, onReady, { graceMs = 1500 } = {}) {
  let done = false
  const startedAt = Date.now()
  let graceTimer = null

  function check() {
    if (done || media.seeking) return
    const enough = media.readyState >= HAVE_ENOUGH_DATA
    const some = media.readyState >= HAVE_FUTURE_DATA && Date.now() - startedAt >= graceMs
    if (enough || some) {
      cancel()
      onReady()
    }
  }

  function cancel() {
    done = true
    clearTimeout(graceTimer)
    for (const e of EVENTS) media.removeEventListener(e, check)
  }

  for (const e of EVENTS) media.addEventListener(e, check)
  graceTimer = setTimeout(check, graceMs)
  // A just-issued seek may not have flipped `seeking` yet; check on the next tick
  setTimeout(check, 0)
  return cancel
}
