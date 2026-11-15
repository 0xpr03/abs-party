import { ref, watch, onScopeDispose } from 'vue'

// Keeps the screen on while `enabled` is true. Browsers release the lock whenever the page
// is hidden (tab switch, screen locked by hand), so it's re-requested on every return.
export function useWakeLock(enabled) {
  const supported = typeof navigator !== 'undefined' && 'wakeLock' in navigator
  const active = ref(false)
  let sentinel = null
  let requesting = false

  async function acquire() {
    if (!supported || !enabled.value || sentinel || requesting) return
    if (document.visibilityState !== 'visible') return
    requesting = true
    try {
      const lock = await navigator.wakeLock.request('screen')
      lock.addEventListener('release', () => {
        if (sentinel === lock) { sentinel = null; active.value = false }
      })
      sentinel = lock
      active.value = true
      // Switched off while the request was pending
      if (!enabled.value) release()
    } catch (e) {
      // e.g. battery saver, or Safari wanting a user gesture first; acquire() is retried
      // on the next visibility change or user interaction
      console.warn('Screen wake lock refused:', e)
    } finally {
      requesting = false
    }
  }

  function release() {
    const lock = sentinel
    sentinel = null
    active.value = false
    lock?.release().catch(() => {})
  }

  function onVisibilityChange() {
    if (document.visibilityState === 'visible') acquire()
  }

  watch(enabled, (on) => (on ? acquire() : release()), { immediate: true })
  document.addEventListener('visibilitychange', onVisibilityChange)
  onScopeDispose(() => {
    document.removeEventListener('visibilitychange', onVisibilityChange)
    release()
  })

  return { supported, active, acquire }
}
