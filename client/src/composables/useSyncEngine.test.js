import { describe, it, expect, vi } from 'vitest'
import { ref } from 'vue'
import { useSyncEngine } from './useSyncEngine.js'

function makePlayer(initialTime = 0) {
  return {
    currentTime: ref(initialTime),
    isPlaying: ref(false),
    play: vi.fn(),
    pause: vi.fn(),
    seekTo: vi.fn(),
    setSpeed: vi.fn(),
  }
}

describe('useSyncEngine', () => {
  it('ignores all commands when caller is host', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    handleMessage({ type: 'play', position: 100 }, true)
    handleMessage({ type: 'pause', position: 100 }, true)
    handleMessage({ type: 'seek', position: 100 }, true)

    expect(player.play).not.toHaveBeenCalled()
    expect(player.pause).not.toHaveBeenCalled()
    expect(player.seekTo).not.toHaveBeenCalled()
  })

  it('applies play with latency compensation', () => {
    const player = makePlayer()
    const rtt = ref(400) // 400ms → 0.2s offset
    const { handleMessage } = useSyncEngine(player, rtt)

    handleMessage({ type: 'play', position: 100 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(100.2)
    expect(player.play).toHaveBeenCalled()
  })

  it('applies play with zero latency when rtt is 0', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    handleMessage({ type: 'play', position: 50 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(50)
    expect(player.play).toHaveBeenCalled()
  })

  it('applies pause at given position', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    handleMessage({ type: 'pause', position: 77.5 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(77.5)
    expect(player.pause).toHaveBeenCalled()
  })

  it('applies seek directly', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    handleMessage({ type: 'seek', position: 200 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(200)
    expect(player.play).not.toHaveBeenCalled()
  })

  it('applies speed change', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    handleMessage({ type: 'speed', rate: 1.5 }, false)

    expect(player.setSpeed).toHaveBeenCalledWith(1.5)
  })

  it('sync does not seek when drift is within threshold', () => {
    const player = makePlayer(100)
    const { handleMessage } = useSyncEngine(player, ref(0))

    // 101 vs 100 = 1s drift, below 2.5s threshold
    handleMessage({ type: 'sync', position: 101 }, false)

    expect(player.seekTo).not.toHaveBeenCalled()
  })

  it('sync hard-seeks when drift exceeds threshold', () => {
    const player = makePlayer(100)
    const { handleMessage } = useSyncEngine(player, ref(0))

    // 107 vs 100 = 7s drift, above 2.5s threshold
    handleMessage({ type: 'sync', position: 107 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(107)
  })

  it('sync handles negative drift (guest ahead of host)', () => {
    const player = makePlayer(110)
    const { handleMessage } = useSyncEngine(player, ref(0))

    // guest is 10s ahead — should seek back
    handleMessage({ type: 'sync', position: 100 }, false)

    expect(player.seekTo).toHaveBeenCalledWith(100)
  })

  it('unknown message type does nothing', () => {
    const player = makePlayer()
    const { handleMessage } = useSyncEngine(player, ref(0))

    // Should not throw
    handleMessage({ type: 'room_state', position: 0 }, false)

    expect(player.play).not.toHaveBeenCalled()
    expect(player.seekTo).not.toHaveBeenCalled()
  })
})
