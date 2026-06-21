// REST API calls go through our own backend proxy at /api/abs-proxy to avoid CORS issues.
// absBase is still passed for caller convenience but is unused here — it remains needed
// by components that build direct media URLs (cover art, audio streams).

const PROXY = '/api/abs-proxy'

function authHeaders(token) {
  return { Authorization: `Bearer ${token}` }
}

export async function login(username, password) {
  const resp = await fetch('/api/abs/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  if (!resp.ok) throw new Error('Login failed')
  return resp.json()
}

export async function getMe(_absBase, token) {
  const resp = await fetch(`${PROXY}/api/me`, { headers: authHeaders(token) })
  if (!resp.ok) throw new Error('Failed to fetch user')
  return resp.json()
}

export async function getLibraries(_absBase, token) {
  const resp = await fetch(`${PROXY}/api/libraries`, { headers: authHeaders(token) })
  if (!resp.ok) throw new Error('Failed to fetch libraries')
  return resp.json()
}

export async function getLibraryItems(_absBase, token, libraryId, page = 0, sort = 'progress', desc = true) {
  const params = new URLSearchParams({ limit: 40, page, mediaType: 'book' })
  if (sort) { params.set('sort', sort); params.set('desc', desc ? 1 : 0) }
  const resp = await fetch(
    `${PROXY}/api/libraries/${libraryId}/items?${params}`,
    { headers: authHeaders(token) }
  )
  if (!resp.ok) throw new Error('Failed to fetch items')
  return resp.json()
}

export async function getItem(_absBase, token, itemId) {
  const resp = await fetch(`${PROXY}/api/items/${itemId}?expanded=1`, { headers: authHeaders(token) })
  if (!resp.ok) throw new Error('Failed to fetch item')
  return resp.json()
}

export async function getProgress(_absBase, token, libraryItemId) {
  const resp = await fetch(`${PROXY}/api/me/progress/${libraryItemId}`, { headers: authHeaders(token) })
  if (resp.status === 404) return null
  if (!resp.ok) throw new Error('Failed to fetch progress')
  return resp.json()
}

export async function getBookmarks(_absBase, token, libraryItemId) {
  const me = await getMe(_absBase, token)
  return (me.bookmarks || []).filter(b => b.libraryItemId === libraryItemId)
}

export async function createBookmark(_absBase, token, libraryItemId, time, title) {
  const resp = await fetch(`${PROXY}/api/me/item/${libraryItemId}/bookmark`, {
    method: 'POST',
    headers: { ...authHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({ time, title }),
  })
  if (!resp.ok) throw new Error('Failed to create bookmark')
  return resp.json()
}

export async function startPlaybackSession(_absBase, token, libraryItemId) {
  const resp = await fetch(`${PROXY}/api/items/${libraryItemId}/play`, {
    method: 'POST',
    headers: { ...authHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({
      mediaPlayer: 'html5',
      supportedMimeTypes: ['audio/mpeg', 'audio/mp4', 'audio/ogg', 'audio/webm', 'audio/flac'],
      deviceInfo: { deviceId: getDeviceId(), deviceName: 'ABS Party Web' },
      forceDirectPlay: false,
    }),
  })
  if (!resp.ok) throw new Error('Failed to start playback session')
  return resp.json()
}

export async function syncProgress(_absBase, token, sessionId, currentTime, timeListened) {
  await fetch(`${PROXY}/api/session/${sessionId}/sync`, {
    method: 'POST',
    headers: { ...authHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({ currentTime, timeListened }),
  })
}

export async function closeSession(_absBase, token, sessionId, currentTime, timeListened) {
  await fetch(`${PROXY}/api/session/${sessionId}/close`, {
    method: 'POST',
    headers: { ...authHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({ currentTime, timeListened }),
  })
}

function getDeviceId() {
  let id = localStorage.getItem('absPartyDeviceId')
  if (!id) {
    id = crypto.randomUUID()
    localStorage.setItem('absPartyDeviceId', id)
  }
  return id
}
