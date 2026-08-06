<template>
  <div class="room-page">
    <!-- Error / loading states -->
    <div v-if="fatalError" class="fatal">
      <p>{{ fatalError }}</p>
      <button @click="router.push('/')">Go home</button>
    </div>

    <template v-else>
      <!-- Header bar -->
      <div class="room-header">
        <div class="room-meta">
          <span class="room-title">{{ roomState.item?.title || 'Connecting…' }}</span>
          <span class="room-author">{{ roomState.item?.author }}</span>
        </div>
        <div class="room-code">
          Room: <strong>{{ roomId }}</strong>
          <button class="btn-copy" @click="copyLink" title="Copy invite link">📋</button>
        </div>
        <button class="btn-leave" @click="leaveRoom">Leave</button>
      </div>

      <!-- Main content: player + sidebar -->
      <div class="room-body">
        <!-- Player -->
        <div class="player-area">
          <div v-if="bookmarkSaved" class="notice">
            ✅ Bookmark saved at your previous position — you can restore it from the ABS bookmarks panel.
          </div>
          <div v-if="preparing" class="status">Opening playback session…</div>

          <div v-else class="player">
            <!-- Cover -->
            <div class="cover-wrap">
              <img
                v-if="roomState.item?.id"
                :src="`${absBase}/api/items/${roomState.item.id}/cover`"
                class="cover"
                alt="cover"
              />
            </div>

            <!-- Progress bar -->
            <div class="progress-wrap">
              <span class="time">{{ fmt(player.currentTime.value) }}</span>
              <input
                type="range"
                class="progress"
                :value="player.currentTime.value"
                :max="player.duration.value || 1"
                step="1"
                @input="onSeek"
              />
              <span class="time">{{ fmt(player.duration.value) }}</span>
            </div>

            <!-- Controls -->
            <div class="controls">
              <button class="ctrl-btn" @click="skip(-10)">-10s</button>
              <button class="ctrl-btn play-btn" :class="{ buffering: buffering }" @click="togglePlay">
                {{ buffering ? '⏳' : player.isPlaying.value ? '⏸' : '▶' }}
              </button>
              <button class="ctrl-btn" @click="skip(10)">+10s</button>
            </div>
            <div v-if="buffering" class="buffering-notice">Buffering…</div>

            <div class="volume-row">
              <label class="volume-label">Vol
                <input
                  type="range"
                  class="volume-slider"
                  :value="player.volume.value"
                  min="0" max="1" step="0.05"
                  @input="e => player.setVolume(Number(e.target.value))"
                />
              </label>
            </div>
          </div>
        </div>

        <!-- Participants + Bookmarks sidebar -->
        <div class="sidebar">
          <h3>Listeners ({{ participants.length }})</h3>
          <ul class="participant-list">
            <li v-for="p in participants" :key="p.name" :class="{ host: p.is_host }">
              {{ p.name }}<span v-if="p.is_host" class="host-badge">host</span>
            </li>
          </ul>

          <div class="sidebar-divider"></div>

          <h3>Bookmarks</h3>
          <div class="bookmark-add">
            <input
              v-model="newBookmarkTitle"
              class="bookmark-input"
              placeholder="Title (optional)"
              @keyup.enter="addBookmark"
            />
            <button class="btn-bookmark-add" @click="addBookmark">Add</button>
          </div>
          <ul class="bookmark-list">
            <li v-for="b in bookmarks" :key="b.time"
                class="bookmark-item bookmark-seekable"
                @click="seekToBookmark(b)">
              <span class="bookmark-title">{{ b.title }}</span>
              <span class="bookmark-time">{{ fmt(b.time) }}</span>
            </li>
            <li v-if="bookmarks.length === 0" class="bookmark-empty">No bookmarks yet</li>
          </ul>
        </div>
      </div>

      <!-- Event log -->
      <div v-if="eventLog.length > 0" class="event-log">
        <div v-for="(e, i) in eventLog" :key="i" class="event-entry" :class="`event-${e.level}`">
          <span class="event-time">{{ e.time }}</span>
          <span class="event-msg">{{ e.message }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup>
import { ref, reactive, computed, inject, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useWebSocket } from '../composables/useWebSocket.js'
import { useAbsPlayer } from '../composables/useAbsPlayer.js'
import { useSyncEngine } from '../composables/useSyncEngine.js'
import { getBookmarks, createBookmark } from '../api/abs.js'
import { useEventLog } from '../composables/useEventLog.js'
import { useAudioCues } from '../composables/useAudioCues.js'

const route = useRoute()
const router = useRouter()
const auth = inject('auth')
const absBase = inject('ABS_BASE')

const roomId = ref(route.params.id)
const isHostMode = route.query.host === 'true'  // constant: this tab intends to act as room creator
const displayName = computed(() => auth.username)

const fatalError = ref('')
const preparing = ref(true)
const bookmarkSaved = ref(false)
const buffering = ref(false)
const participants = ref([])
const bookmarks = ref([])
const newBookmarkTitle = ref('')
const roomState = reactive({ item: null, position: 0, playing: false, speed: 1 })
const isHost = ref(isHostMode)

let bufferingTimeout = null
let bufferingReadyCb = null

let hasConnectedOnce = false

const player = useAbsPlayer(absBase)

const { events: eventLog, addEvent } = useEventLog()
const { playJoinLeave, playAlert } = useAudioCues(player.volume)

const { connect, send, connected, rtt } = useWebSocket(handleMessage)
const { handleMessage: applySyncMsg } = useSyncEngine(player, rtt, () => {
  addEvent('Desync detected — correcting…', 'alert')
  playAlert()
})

watch(connected, (val, old) => {
  if (old && !val) {
    participants.value = []
    addEvent('Connection lost — reconnecting…', 'alert')
    playAlert()
  } else if (!old && val) {
    if (hasConnectedOnce) {
      addEvent('Reconnected', 'normal')
      playAlert()
      rejoinRoom()
    }
    hasConnectedOnce = true
  }
})

onMounted(async () => {
  if (!auth.token) {
    fatalError.value = 'You must be signed in to join a room.'
    return
  }

  if (isHostMode && !route.query.itemId) {
    fatalError.value = 'No book selected. Please go back and pick a book from the library first.'
    return
  }

  connect()

  // Wait for WS to connect, then send join/create
  const waitForConnection = () => new Promise((resolve) => {
    const check = setInterval(() => {
      if (connected.value) { clearInterval(check); resolve() }
    }, 100)
    setTimeout(() => { clearInterval(check); resolve() }, 8000)
  })

  await waitForConnection()

  if (!connected.value) {
    fatalError.value = 'Could not connect to the party server.'
    return
  }

  if (isHostMode) {
    send({
      type: 'create_room',
      room_id: roomId.value,
      abs_token: auth.token,
      item_id: route.query.itemId,
      item_title: route.query.title,
      item_author: route.query.author,
      library_id: route.query.libraryId,
    })
  } else {
    send({
      type: 'join',
      room_id: roomId.value,
      abs_token: auth.token,
    })
  }
})

async function handleMessage(msg) {
  if (msg.type === 'room_state') {
    roomState.item = msg.item
    roomState.position = msg.position
    roomState.playing = msg.playing
    roomState.speed = msg.speed
    participants.value = msg.participants

    const me = participants.value.find(p => p.name === displayName.value)
    isHost.value = me?.is_host ?? false

    // Now open the player (saves bookmark, creates ABS session)
    if (preparing.value && auth.token && roomState.item?.id) {
      preparing.value = false
      try {
        const session = await player.open(auth.token, roomState.item.id)
        bookmarkSaved.value = true
        setTimeout(() => { bookmarkSaved.value = false }, 5000)
        await loadBookmarks()
        if (isHostMode) {
          // Host: start from their own last ABS position and publish it so guests sync correctly
          const startPos = Math.max(0, session.currentTime ?? 0)
          player.seekTo(startPos)
          send({ type: 'seek', position: startPos })
        } else {
          // Guest: follow the room's canonical position
          player.seekTo(roomState.position)
          if (roomState.playing) player.play()
        }
      } catch (e) {
        fatalError.value = 'Failed to open playback session: ' + e.message
      }
    } else {
      // Reconnect: player already open — resync state
      if (isHostMode) {
        const pos = player.currentTime.value
        send({ type: 'seek', position: pos })
      } else {
        player.seekTo(roomState.position)
        if (roomState.playing) player.play()
        else player.pause()
      }
    }
    return
  }

  if (msg.type === 'participant_joined') {
    participants.value.push({ name: msg.name, is_host: false })
    addEvent(`${msg.name} joined`, 'normal')
    playJoinLeave()
    return
  }
  if (msg.type === 'participant_left') {
    participants.value = participants.value.filter(p => p.name !== msg.name)
    addEvent(`${msg.name} left`, 'normal')
    playJoinLeave()
    return
  }
  if (msg.type === 'host_changed') {
    participants.value = participants.value.map(p => ({ ...p, is_host: p.name === msg.name }))
    isHost.value = msg.name === displayName.value
    return
  }
  if (msg.type === 'error') {
    fatalError.value = msg.message
    return
  }

  if (msg.type === 'play_intent') {
    const initiator = msg.sender_name || 'Someone'
    addEvent(`${initiator} starting playback — buffering…`, 'normal')
    cancelBuffering()
    buffering.value = true
    player.seekTo(msg.position)
    bufferingReadyCb = () => sendReady()
    player.audio.addEventListener('canplaythrough', bufferingReadyCb, { once: true })
    bufferingTimeout = setTimeout(sendReady, 20_000)
    return
  }

  if (msg.type === 'play') {
    const label = msg.sender_name ? `${msg.sender_name} resumed playback` : 'All ready — playing'
    addEvent(label, 'normal')
    cancelBuffering()
  }
  if (msg.type === 'pause') {
    addEvent(`${msg.sender_name} paused playback`, 'normal')
    cancelBuffering()
  }
  if (msg.type === 'seek')  addEvent(`${msg.sender_name} seeked to ${fmt(msg.position)}`, 'normal')

  applySyncMsg(msg)
}

function sendReady() {
  cancelBuffering()
  send({ type: 'ready' })
}

function cancelBuffering() {
  buffering.value = false
  clearTimeout(bufferingTimeout)
  bufferingTimeout = null
  if (bufferingReadyCb) {
    player.audio.removeEventListener('canplaythrough', bufferingReadyCb)
    bufferingReadyCb = null
  }
}

function rejoinRoom() {
  if (isHostMode && roomState.item?.id) {
    send({
      type: 'create_room',
      room_id: roomId.value,
      abs_token: auth.token,
      item_id: roomState.item.id,
      item_title: roomState.item.title,
      item_author: roomState.item.author,
      library_id: roomState.item.library_id,
    })
  } else if (roomId.value) {
    send({ type: 'join', room_id: roomId.value, abs_token: auth.token })
  }
}

function togglePlay() {
  const pos = player.currentTime.value
  if (player.isPlaying.value) {
    player.pause()
    send({ type: 'pause', position: pos })
    addEvent('You paused playback', 'normal')
  } else {
    send({ type: 'play_intent', position: pos })
    addEvent('You started playback — waiting for all to buffer…', 'normal')
  }
}

function onSeek(evt) {
  const pos = Number(evt.target.value)
  player.seekTo(pos)
  send({ type: 'seek', position: pos })
  addEvent(`You seeked to ${fmt(pos)}`, 'normal')
}

function skip(seconds) {
  const pos = Math.max(0, player.currentTime.value + seconds)
  player.seekTo(pos)
  send({ type: 'seek', position: pos })
  addEvent(`You seeked to ${fmt(pos)}`, 'normal')
}

function seekToBookmark(b) {
  player.seekTo(b.time)
  send({ type: 'seek', position: b.time })
  addEvent(`You seeked to ${fmt(b.time)}`, 'normal')
}

async function loadBookmarks() {
  if (!roomState.item?.id) return
  try {
    const data = await getBookmarks(null, auth.token, roomState.item.id)
    bookmarks.value = Array.isArray(data) ? data.sort((a, b) => b.time - a.time) : []
  } catch (e) {
    console.warn('Failed to load bookmarks:', e)
  }
}

async function addBookmark() {
  if (!roomState.item?.id) return
  const time = player.currentTime.value
  const ts = new Date().toISOString().slice(0, 19)
  const title = newBookmarkTitle.value.trim() || ts
  try {
    await createBookmark(null, auth.token, roomState.item.id, time, title)
    newBookmarkTitle.value = ''
    await loadBookmarks()
  } catch (e) {
    console.warn('Failed to create bookmark:', e)
  }
}

async function leaveRoom() {
  await player.close()
  router.push('/')
}

function copyLink() {
  navigator.clipboard.writeText(`${location.origin}/room/${roomId.value}`)
}

function fmt(s) {
  if (!s || isNaN(s)) return '0:00'
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = Math.floor(s % 60)
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
  return `${m}:${String(sec).padStart(2, '0')}`
}
</script>

<style scoped>
.room-page { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
.room-header { display: flex; align-items: center; gap: 1rem; padding: 0.75rem 1.5rem; background: #16213e; border-bottom: 1px solid #0f3460; flex-wrap: wrap; }
.room-meta { flex: 1; }
.room-title { font-weight: 700; font-size: 1rem; color: #e0e0e0; }
.room-author { display: block; font-size: 0.8rem; color: #888; }
.room-code { font-size: 0.85rem; color: #aaa; display: flex; align-items: center; gap: 0.4rem; }
.btn-copy { background: none; border: none; cursor: pointer; font-size: 1rem; line-height: 1; }
.btn-leave { background: #d63031; border: none; border-radius: 6px; color: #fff; padding: 0.4rem 1rem; cursor: pointer; font-size: 0.85rem; }

.room-body { display: flex; flex: 1; overflow: hidden; min-height: 0; }
.player-area { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 2rem; overflow-y: auto; }
.sidebar { width: 220px; border-left: 1px solid #0f3460; padding: 1rem; overflow-y: auto; background: #16213e; }

.notice { background: #00b894; color: #fff; border-radius: 8px; padding: 0.75rem 1rem; margin-bottom: 1rem; font-size: 0.85rem; max-width: 500px; }
.status { color: #888; margin: 3rem 0; }

.player { display: flex; flex-direction: column; align-items: center; gap: 1.5rem; width: 100%; max-width: 500px; }
.cover-wrap { width: 220px; height: 220px; border-radius: 12px; overflow: hidden; background: #0f3460; }
.cover { width: 100%; height: 100%; object-fit: cover; }

.progress-wrap { display: flex; align-items: center; gap: 0.75rem; width: 100%; }
.progress { flex: 1; accent-color: #6c5ce7; }
.time { font-size: 0.8rem; color: #888; white-space: nowrap; min-width: 45px; }

.controls { display: flex; gap: 1rem; align-items: center; }
.ctrl-btn { background: #16213e; border: 1px solid #0f3460; color: #ccc; padding: 0.6rem 1.2rem; border-radius: 8px; cursor: pointer; font-size: 0.9rem; }
.ctrl-btn:disabled { opacity: 0.4; cursor: default; }
.play-btn { font-size: 1.5rem; padding: 0.75rem 1.5rem; background: #6c5ce7; border-color: #6c5ce7; color: #fff; border-radius: 50%; width: 64px; height: 64px; display: flex; align-items: center; justify-content: center; }
.play-btn.buffering { background: #fdcb6e; border-color: #fdcb6e; color: #2d3436; }
.buffering-notice { font-size: 0.8rem; color: #fdcb6e; }

.volume-row { display: flex; align-items: center; justify-content: center; }
.volume-label { display: flex; align-items: center; gap: 0.5rem; font-size: 0.85rem; color: #aaa; white-space: nowrap; }
.volume-slider { width: 90px; accent-color: #6c5ce7; cursor: pointer; }

.sidebar h3 { margin: 0 0 0.75rem; font-size: 0.9rem; color: #aaa; }
.sidebar-divider { border: none; border-top: 1px solid #0f3460; margin: 1rem 0; }
.bookmark-add { display: flex; gap: 0.4rem; margin-bottom: 0.6rem; }
.bookmark-input { flex: 1; min-width: 0; padding: 0.3rem 0.5rem; background: #0f3460; border: 1px solid #1a4a80; border-radius: 4px; color: #fff; font-size: 0.8rem; }
.bookmark-input::placeholder { color: #555; }
.btn-bookmark-add { background: #6c5ce7; border: none; border-radius: 4px; color: #fff; padding: 0.3rem 0.6rem; cursor: pointer; font-size: 0.8rem; white-space: nowrap; }
.bookmark-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0.35rem; }
.bookmark-item { display: flex; justify-content: space-between; align-items: baseline; gap: 0.4rem; font-size: 0.8rem; }
.bookmark-title { color: #ccc; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
.bookmark-time { color: #6c5ce7; white-space: nowrap; flex-shrink: 0; font-variant-numeric: tabular-nums; }
.bookmark-empty { font-size: 0.8rem; color: #555; }
.bookmark-seekable { cursor: pointer; }
.bookmark-seekable:hover .bookmark-time { color: #a29bfe; }
.participant-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0.5rem; }
.participant-list li { font-size: 0.9rem; color: #e0e0e0; display: flex; align-items: center; gap: 0.5rem; }
.participant-list li.host { color: #a29bfe; font-weight: 600; }
.host-badge { font-size: 0.7rem; background: #6c5ce7; color: #fff; padding: 0.15rem 0.4rem; border-radius: 4px; }
.fatal { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100vh; gap: 1rem; color: #ff7675; }
.fatal button { background: #6c5ce7; border: none; border-radius: 6px; color: #fff; padding: 0.6rem 1.5rem; cursor: pointer; }

.event-log { background: #080815; border-top: 1px solid #0f3460; padding: 0.35rem 1rem; font-family: monospace; font-size: 0.72rem; display: flex; flex-direction: column; gap: 0.1rem; }
.event-entry { display: flex; gap: 0.75rem; }
.event-time { color: #444; flex-shrink: 0; }
.event-normal .event-msg { color: #778; }
.event-alert .event-msg { color: #e17055; }
</style>
