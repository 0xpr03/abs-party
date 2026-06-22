let ctx = null

function getCtx() {
  if (!ctx) ctx = new AudioContext()
  return ctx
}

export function useAudioCues(volume) {
  function tone(freq, start, duration) {
    const c = getCtx()
    const vol = (volume?.value ?? 1) * 0.35
    const osc = c.createOscillator()
    const gain = c.createGain()
    osc.connect(gain)
    gain.connect(c.destination)
    osc.frequency.value = freq
    gain.gain.setValueAtTime(vol, c.currentTime + start)
    gain.gain.exponentialRampToValueAtTime(0.001, c.currentTime + start + duration)
    osc.start(c.currentTime + start)
    osc.stop(c.currentTime + start + duration)
  }

  function playJoinLeave() {
    tone(880, 0, 0.18)
  }

  function playAlert() {
    tone(660, 0, 0.2)
    tone(440, 0.2, 0.3)
  }

  return { playJoinLeave, playAlert }
}
