import { describe, it, expect } from 'vitest'
import { createRoomClock } from './roomClock.js'

function fakeNow() {
  let t = 0
  const now = () => t
  now.advance = (ms) => { t += ms }
  return now
}

describe('createRoomClock', () => {
  it('stays put while paused', () => {
    const now = fakeNow()
    const clock = createRoomClock(now)
    clock.set({ position: 50, playing: false })
    now.advance(5000)
    expect(clock.current()).toBe(50)
  })

  it('advances while playing, scaled by speed', () => {
    const now = fakeNow()
    const clock = createRoomClock(now)
    clock.set({ position: 50, playing: true, speed: 1.5 })
    now.advance(2000)
    expect(clock.current()).toBe(53)
  })

  it('seek while playing keeps the clock running from the new position', () => {
    const now = fakeNow()
    const clock = createRoomClock(now)
    clock.set({ position: 0, playing: true })
    now.advance(10_000)
    clock.set({ position: 100 })
    now.advance(1000)
    expect(clock.current()).toBe(101)
    expect(clock.isPlaying()).toBe(true)
  })

  it('pausing freezes at the extrapolated position when none is given', () => {
    const now = fakeNow()
    const clock = createRoomClock(now)
    clock.set({ position: 10, playing: true })
    now.advance(3000)
    clock.set({ playing: false })
    now.advance(3000)
    expect(clock.current()).toBe(13)
  })
})
