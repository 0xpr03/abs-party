import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { effectScope, ref, nextTick } from 'vue'
import { useMediaSession } from './useMediaSession.js'

describe('useMediaSession', () => {
  let session, handlers, actions, player, scope

  beforeEach(() => {
    handlers = {}
    session = {
      metadata: null,
      playbackState: 'none',
      setActionHandler: (a, fn) => { handlers[a] = fn },
      setPositionState: vi.fn(),
    }
    vi.stubGlobal('navigator', { mediaSession: session })
    vi.stubGlobal('MediaMetadata', class { constructor(init) { Object.assign(this, init) } })
    actions = { play: vi.fn(), pause: vi.fn(), skip: vi.fn(), seekTo: vi.fn() }
    player = { isPlaying: ref(false), currentTime: ref(30), duration: ref(100), audio: new EventTarget() }
    player.audio.playbackRate = 1
    scope = effectScope()
  })

  afterEach(() => {
    scope.stop()
    vi.unstubAllGlobals()
  })

  it('routes media keys to room actions', () => {
    scope.run(() => useMediaSession(() => null, player, actions))
    handlers.play()
    handlers.pause()
    handlers.seekbackward({})
    handlers.seekforward({ seekOffset: 30 })
    handlers.seekto({ seekTime: 42 })
    expect(actions.play).toHaveBeenCalled()
    expect(actions.pause).toHaveBeenCalled()
    expect(actions.skip.mock.calls).toEqual([[-10], [30]])
    expect(actions.seekTo).toHaveBeenCalledWith(42)
  })

  it('shows the room item and playback state', async () => {
    const item = ref(null)
    scope.run(() => useMediaSession(() => item.value, player, actions))
    item.value = { id: 'abc', title: 'Book', author: 'Writer' }
    player.isPlaying.value = true
    await nextTick()
    expect(session.metadata).toMatchObject({ title: 'Book', artist: 'Writer' })
    expect(session.metadata.artwork[0].src).toMatch(/\/api\/abs-proxy\/api\/items\/abc\/cover$/)
    expect(session.playbackState).toBe('playing')
    expect(session.setPositionState).toHaveBeenLastCalledWith({ duration: 100, position: 30, playbackRate: 1 })
  })

  it('clears everything when the room is left', () => {
    scope.run(() => useMediaSession(() => ({ id: 'abc', title: 'Book' }), player, actions))
    scope.stop()
    expect(handlers.play).toBeNull()
    expect(session.metadata).toBeNull()
    expect(session.playbackState).toBe('none')
  })
})
