import { describe, it, expect } from 'vitest'
import { shortenTitle } from './title.js'

describe('shortenTitle', () => {
  // Simple titles — nothing to shorten
  it('returns a plain single word unchanged', () => {
    expect(shortenTitle('Bread')).toBe('Bread')
  })

  it('returns a plain multi-word title unchanged', () => {
    expect(shortenTitle('The Wandering Fox')).toBe('The Wandering Fox')
  })

  it('preserves a number already in the main title', () => {
    expect(shortenTitle('Ironclad 7')).toBe('Ironclad 7')
  })

  // Trailing parentheticals
  it('strips a single-word trailing parenthetical', () => {
    expect(shortenTitle('Skyward (Abridged)')).toBe('Skyward')
  })

  it('strips a multi-word trailing parenthetical', () => {
    expect(shortenTitle('The Iron Chalice (Dramatized Adaptation)')).toBe('The Iron Chalice')
  })

  it('strips a trailing parenthetical that follows a number in the main title', () => {
    expect(shortenTitle('Stormwatch 3 (Unabridged)')).toBe('Stormwatch 3')
  })

  // Colon-separated subtitles
  it('drops a pure-text subtitle with no digit', () => {
    expect(shortenTitle('Ember Falls: A Dark Fantasy Epic')).toBe('Ember Falls')
  })

  it('keeps a subtitle segment that contains a digit', () => {
    expect(shortenTitle('Ember Falls: Book 2')).toBe('Ember Falls: Book 2')
  })

  it('drops descriptive middle subtitle and keeps volume at the end', () => {
    expect(shortenTitle('Frostfire: The Saga of the North: Volume 4')).toBe('Frostfire: Volume 4')
  })

  it('keeps multiple subtitle segments that each contain a digit', () => {
    expect(shortenTitle('Galepath: Season 2: Episode 10')).toBe('Galepath: Season 2: Episode 10')
  })

  it('drops all subtitle segments when none contain a digit', () => {
    expect(shortenTitle('Wraithsong: An Endless Journey: Tales Untold')).toBe('Wraithsong')
  })

  // Combined: parenthetical + subtitle
  it('strips trailing parenthetical then drops digit-free subtitle', () => {
    expect(shortenTitle('Ashcroft: Rise of the Pale (Unabridged)')).toBe('Ashcroft')
  })

  it('strips trailing parenthetical and keeps volume subtitle', () => {
    expect(shortenTitle('Ashcroft: Part 3 (Unabridged)')).toBe('Ashcroft: Part 3')
  })

  it('handles parenthetical after a multi-colon title with a volume', () => {
    expect(shortenTitle('Moonriven: The Lost Age: Volume 2 (Abridged)')).toBe('Moonriven: Volume 2')
  })
})
