import { watch, onScopeDispose } from 'vue'

// Lock screen / notification / headset controls. Without our own handlers the browser wires
// these straight to the <audio> element, which pauses only this listener while the room
// plays on; here they act on the room like the on-page buttons.
export function useMediaSession(getItem, player, actions) {
  if (typeof navigator === 'undefined' || !('mediaSession' in navigator)) return
  const session = navigator.mediaSession

  const handlers = {
    play: () => actions.play(),
    pause: () => actions.pause(),
    seekbackward: (d) => actions.skip(-(d?.seekOffset || 10)),
    seekforward: (d) => actions.skip(d?.seekOffset || 10),
    seekto: (d) => { if (Number.isFinite(d?.seekTime)) actions.seekTo(d.seekTime) },
  }
  for (const [action, handler] of Object.entries(handlers)) setHandler(action, handler)

  watch(getItem, (item) => {
    session.metadata = item && typeof MediaMetadata === 'function'
      ? new MediaMetadata({
          title: item.title,
          artist: item.author,
          album: 'ABS Party',
          artwork: [{ src: `${location.origin}/api/abs-proxy/api/items/${item.id}/cover` }],
        })
      : null
  }, { immediate: true })

  watch(player.isPlaying, (playing) => {
    session.playbackState = playing ? 'playing' : 'paused'
    updatePosition()
  }, { immediate: true })

  // The OS extrapolates the position itself; it only needs telling when that breaks
  watch(player.duration, updatePosition)
  player.audio.addEventListener('seeked', updatePosition)
  player.audio.addEventListener('ratechange', updatePosition)

  function updatePosition() {
    const duration = player.duration.value
    if (!session.setPositionState || !Number.isFinite(duration) || duration <= 0) return
    try {
      session.setPositionState({
        duration,
        position: Math.min(Math.max(0, player.currentTime.value), duration),
        playbackRate: player.audio.playbackRate || 1,
      })
    } catch {}
  }

  function setHandler(action, handler) {
    // Browsers throw for actions they don't know
    try { session.setActionHandler(action, handler) } catch {}
  }

  onScopeDispose(() => {
    player.audio.removeEventListener('seeked', updatePosition)
    player.audio.removeEventListener('ratechange', updatePosition)
    for (const action of Object.keys(handlers)) setHandler(action, null)
    session.metadata = null
    session.playbackState = 'none'
  })
}
