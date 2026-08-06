export function shortenTitle(title) {
  // Strip trailing parenthetical annotations, e.g. "(Unabridged)", "(Boxed Set)"
  let s = title.replace(/\s*\([^)]+\)\s*$/, '').trim()
  // For colon-separated parts: keep main title + any segments containing a digit (volume numbers)
  const parts = s.split(':').map(p => p.trim())
  if (parts.length > 1) {
    const kept = [parts[0], ...parts.slice(1).filter(p => /\d/.test(p))]
    s = kept.join(': ')
  }
  return s
}
