import { ref, onUnmounted } from 'vue'
import Hls from 'hls.js'
import { startPlaybackSession, syncProgress, closeSession, getProgress, createBookmark } from '../api/abs.js'

const SYNC_INTERVAL_MS = 10_000

export function useAbsPlayer(absBase) {
  const audio = new Audio()
  const sessionId = ref(null)
  const currentTime = ref(0)
  const duration = ref(0)
  const isPlaying = ref(false)
  const saved = parseFloat(localStorage.getItem('absPartyVolume'))
  if (isFinite(saved) && saved >= 0 && saved <= 1) {
    audio.volume = saved
  }
  const volume = ref(audio.volume)

  let hls = null
  // For direct-play multi-track books: all tracks + current index
  let tracks = []
  let trackIndex = 0

  let syncTimer = null
  let listenedSinceSync = 0
  let lastSyncTime = 0
  let currentToken = null
  let currentItemId = null

  audio.addEventListener('timeupdate', () => {
    const offset = tracks[trackIndex]?.startOffset ?? 0
    currentTime.value = offset + audio.currentTime
  })
  audio.addEventListener('durationchange', () => {
    // Report total book duration from the last track's end
    if (tracks.length > 0) {
      const last = tracks[tracks.length - 1]
      duration.value = last.startOffset + last.duration
    } else {
      duration.value = audio.duration
    }
  })
  // Advance to next track when current one finishes (direct-play multi-track)
  audio.addEventListener('ended', () => {
    if (!hls && trackIndex < tracks.length - 1) {
      trackIndex++
      loadDirectTrack(tracks[trackIndex])
      audio.play().catch(() => {})
    } else {
      isPlaying.value = false
    }
  })

  async function open(token, libraryItemId) {
    currentToken = token
    currentItemId = libraryItemId

    await savePreJoinBookmark(token, libraryItemId)

    const session = await startPlaybackSession(null, token, libraryItemId)
    sessionId.value = session.id

    const audioTracks = session.audioTracks ?? []

    // Set duration immediately from session data so the progress bar is correct
    // before the audio element fires durationchange (which only happens on play).
    if (audioTracks.length > 0) {
      const last = audioTracks[audioTracks.length - 1]
      duration.value = (last.startOffset ?? 0) + (last.duration ?? 0)
    } else if (session.duration) {
      duration.value = session.duration
    }

    if (audioTracks.length > 0) {
      const first = audioTracks[0]
      const rawUrl = first.contentUrl?.startsWith('http')
        ? first.contentUrl
        : `${absBase}${first.contentUrl}`

      if (rawUrl.includes('.m3u8')) {
        loadHls(rawUrl)
      } else {
        tracks = audioTracks
        trackIndex = 0
        loadDirectTrack(first)
      }
    }

    startSyncTimer()
    return session
  }

  function loadHls(src) {
    hls?.destroy()
    if (Hls.isSupported()) {
      hls = new Hls()
      hls.loadSource(src)
      hls.attachMedia(audio)
    } else {
      // Safari has native HLS
      audio.src = src
    }
  }

  function loadDirectTrack(track) {
    const base = track.contentUrl.startsWith('http') ? '' : absBase
    const url = `${base}${track.contentUrl}`
    const sep = url.includes('?') ? '&' : '?'
    audio.src = `${url}${sep}token=${encodeURIComponent(currentToken)}`
    audio.load()
  }

  async function savePreJoinBookmark(token, libraryItemId) {
    try {
      const progress = await getProgress(null, token, libraryItemId)
      const time = Math.max(0, progress?.currentTime ?? 0)
      const ts = new Date().toISOString().slice(0, 19)
      await createBookmark(null, token, libraryItemId, time, `pre_party_${ts}`)
    } catch (e) {
      console.warn('Could not save pre-join bookmark:', e)
    }
  }

  function play() {
    audio.play().catch(() => {})
    isPlaying.value = true
  }

  function pause() {
    audio.pause()
    isPlaying.value = false
  }

  function seekTo(seconds) {
    if (hls) {
      audio.currentTime = seconds
    } else if (tracks.length > 1) {
      // Find which track contains this absolute position
      let idx = 0
      for (let i = 0; i < tracks.length; i++) {
        if (seconds < tracks[i].startOffset + tracks[i].duration) {
          idx = i
          break
        }
        idx = i
      }
      const inTrackTime = seconds - (tracks[idx]?.startOffset ?? 0)
      if (idx !== trackIndex) {
        trackIndex = idx
        loadDirectTrack(tracks[trackIndex])
        audio.addEventListener('loadedmetadata', () => {
          audio.currentTime = inTrackTime
        }, { once: true })
      } else {
        audio.currentTime = inTrackTime
      }
    } else {
      audio.currentTime = seconds
    }
    currentTime.value = seconds
  }

  function setSpeed(rate) {
    audio.playbackRate = rate
  }

  function setVolume(v) {
    audio.volume = v
    volume.value = v
    localStorage.setItem('absPartyVolume', v)
  }

  function startSyncTimer() {
    clearInterval(syncTimer)
    lastSyncTime = audio.currentTime

    syncTimer = setInterval(async () => {
      if (!sessionId.value || !currentToken) return
      const now = currentTime.value
      const delta = Math.max(0, now - lastSyncTime)
      listenedSinceSync += isPlaying.value ? delta : 0
      lastSyncTime = now

      if (listenedSinceSync >= 10) {
        try {
          await syncProgress(null, currentToken, sessionId.value, now, listenedSinceSync)
          listenedSinceSync = 0
        } catch (e) {
          console.warn('Progress sync failed:', e)
        }
      }
    }, SYNC_INTERVAL_MS)
  }

  async function close() {
    clearInterval(syncTimer)
    audio.pause()
    hls?.destroy()
    hls = null
    tracks = []
    trackIndex = 0
    if (sessionId.value && currentToken && currentItemId) {
      const time = Math.max(0, currentTime.value)
      const ts = new Date().toISOString().slice(0, 19)
      try {
        await createBookmark(null, currentToken, currentItemId, time, `leave_${ts}`)
      } catch {}
      try {
        await closeSession(null, currentToken, sessionId.value, time, listenedSinceSync)
      } catch {}
    }
    sessionId.value = null
    isPlaying.value = false
  }

  onUnmounted(close)

  return { open, play, pause, seekTo, setSpeed, setVolume, close, currentTime, duration, volume, isPlaying, audio }
}
