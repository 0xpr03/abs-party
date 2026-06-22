// Receives sync commands from the server and applies them to the player.
// Handles latency compensation on play commands using RTT.
// Drift correction: if we're more than DRIFT_THRESHOLD seconds off, hard-seek.

const DRIFT_THRESHOLD = 2.5

export function useSyncEngine(player, rtt, onDesync) {
  function handleMessage(msg) {
    switch (msg.type) {
      case 'play': {
        // Compensate for network latency: advance position by half RTT
        const latencyOffset = (rtt.value || 0) / 2000
        const targetPosition = msg.position + latencyOffset
        player.seekTo(targetPosition)
        player.play()
        break
      }
      case 'pause':
        player.seekTo(msg.position)
        player.pause()
        break
      case 'seek':
        player.seekTo(msg.position)
        break
      case 'speed':
        player.setSpeed(msg.rate)
        break
      case 'sync': {
        const drift = Math.abs(player.currentTime.value - msg.position)
        if (drift > DRIFT_THRESHOLD) {
          player.seekTo(msg.position)
          onDesync?.()
        }
        break
      }
    }
  }

  return { handleMessage }
}
