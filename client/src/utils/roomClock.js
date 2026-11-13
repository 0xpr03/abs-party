// Local model of the room's playback clock, so each client can tell where the room
// *should* be right now (used for late starts and drift correction).

export function createRoomClock(now = () => performance.now()) {
  let position = 0
  let playing = false
  let speed = 1
  let at = now()

  function set(update) {
    position = update.position ?? current()
    if (update.playing !== undefined) playing = update.playing
    if (update.speed !== undefined) speed = update.speed
    at = now()
  }

  function current() {
    if (!playing) return position
    return position + (now() - at) / 1000 * speed
  }

  return { set, current, isPlaying: () => playing }
}
