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
                :src="`/api/abs-proxy/api/items/${roomState.item.id}/cover`"
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
                :disabled="!canControl"
                @input="onSeek"
              />
              <span class="time">{{ fmt(player.duration.value) }}</span>
            </div>

            <!-- Controls -->
            <div class="controls">
              <button class="ctrl-btn" :disabled="!canControl" @click="skip(-10)">-10s</button>
              <button class="ctrl-btn play-btn" :class="{ buffering: buffering }" :disabled="!canControl" @click="togglePlay">
                {{ buffering ? '⏳' : player.isPlaying.value ? '⏸' : '▶' }}
              </button>
              <button class="ctrl-btn" :disabled="!canControl" @click="skip(10)">+10s</button>
            </div>
            <div v-if="!canControl" class="locked-notice">🔒 Only the host can control playback</div>
            <div v-if="buffering" class="buffering-notice">Buffering…</div>
            <button v-if="player.blocked.value" class="btn-unblock" @click="resumeBlockedAudio">
              🔈 Your browser blocked audio. Tap to listen along
            </button>

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

          <h3>Room settings</h3>
          <div class="room-settings">
            <label class="setting" :class="{ readonly: !isHost }">
              <input type="checkbox" :checked="roomSettings.host_only_controls" :disabled="!isHost"
                     @change="e => updateSetting('host_only_controls', e.target.checked)" />
              <span>Only host controls playback</span>
            </label>
            <label class="setting" :class="{ readonly: !isHost }">
              <input type="checkbox" :checked="roomSettings.pause_on_join_leave" :disabled="!isHost"
                     @change="e => updateSetting('pause_on_join_leave', e.target.checked)" />
              <span>Pause on join / leave</span>
            </label>
          </div>

          <div class="sidebar-divider"></div>

          <h3>This device</h3>
          <label class="setting" :class="{ readonly: !wakeLock.supported }">
            <input type="checkbox" :checked="keepAwake" :disabled="!wakeLock.supported"
                   @change="e => setKeepAwake(e.target.checked)" />
            <span>Keep screen on<template v-if="!wakeLock.supported"> (not supported by this browser)</template></span>
          </label>

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
                class="bookmark-item"
                :class="{ 'bookmark-seekable': canControl }"
                @click="canControl && seekToBookmark(b)">
              <span class="bookmark-title">{{ b.title }}</span>
              <span class="bookmark-time">{{ fmt(b.time) }}</span>
            </li>
            <li v-if="bookmarks.length === 0" class="bookmark-empty">No bookmarks yet</li>
          </ul>
        </div>
      </div>

      <!-- Phones suspend pages that aren't on screen -->
      <div v-if="showMobileHint" class="hint-overlay">
        <div class="hint-card">
          <h3>Keep this page open</h3>
          <p>
            To save battery, your phone pauses pages that are in the background or behind a locked screen.
            Keep ABS Party open and the screen on, or you'll drop out of the room and stop hearing what the others do.
          </p>
          <p v-if="wakeLock.supported && keepAwake">
            "Keep screen on" is enabled while you're in a room. You can turn it off in the sidebar.
          </p>
          <label class="setting">
            <input type="checkbox" v-model="hintDontShowAgain" />
            <span>Don't show this again</span>
          </label>
          <button class="hint-ok" @click="closeMobileHint">Got it</button>
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
import { ref, reactive, computed, inject, onMounted, onUnmounted, watch, watchEffect } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useWebSocket } from '../composables/useWebSocket.js'
import { useAbsPlayer } from '../composables/useAbsPlayer.js'
import { useSyncEngine } from '../composables/useSyncEngine.js'
import { getBookmarks, createBookmark } from '../api/abs.js'
import { useEventLog } from '../composables/useEventLog.js'
import { useAudioCues } from '../composables/useAudioCues.js'
import { useWakeLock } from '../composables/useWakeLock.js'
import { useMediaSession } from '../composables/useMediaSession.js'
import { shortenTitle } from '../utils/title.js'
import { createRoomClock } from '../utils/roomClock.js'
import { whenPlayable } from '../utils/playable.js'
import { DEFAULT_ROOM_SETTINGS, loadRoomSettings, saveRoomSettings, normalizeRoomSettings } from '../utils/roomSettings.js'
import { isMobileDevice, loadKeepAwake, saveKeepAwake, mobileHintDismissed, dismissMobileHint } from '../utils/device.js'

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
const roomState = reactive({ item: null })
const isHost = ref(isHostMode)
const roomSettings = reactive({ ...DEFAULT_ROOM_SETTINGS })
const canControl = computed(() => isHost.value || !roomSettings.host_only_controls)
// Where the room should be right now, per the server's last word (see utils/roomClock.js)
const clock = createRoomClock()
// True once our ABS session is open and we've seeked to the room — before that, sync
// messages only update `clock` and the open flow applies them afterwards
let playerReady = false
const DRIFT_CHECK_MS = 2000
const READY_FALLBACK_MS = 20_000


const previousTitle = document.title
watchEffect(() => {
  const bookTitle = roomState.item?.title
  if (bookTitle) {
    const role = isHost.value ? 'Host' : 'Guest'
    document.title = `${shortenTitle(bookTitle)} [${role}] — ABS Party`
  } else {
    document.title = 'ABS Party'
  }
})
onUnmounted(() => { document.title = previousTitle })

let bufferingTimeout = null
let cancelPlayable = null

let hasConnectedOnce = false

const player = useAbsPlayer(absBase)

const isMobile = isMobileDevice()
const keepAwake = ref(loadKeepAwake(isMobile))
const wakeLock = useWakeLock(keepAwake)
const showMobileHint = ref(isMobile && !mobileHintDismissed())
const hintDontShowAgain = ref(false)

useMediaSession(() => roomState.item, player, {
  play: () => {
    if (player.blocked.value) resumeBlockedAudio()
    else if (!player.isPlaying.value) togglePlay()
  },
  pause: () => { if (player.isPlaying.value) togglePlay() },
  skip,
  seekTo: seekRoom,
})

const { events: eventLog, addEvent } = useEventLog()
const { playJoinLeave, playAlert } = useAudioCues(player.volume)

const { connect, send, connected, rtt } = useWebSocket(handleMessage)
const { handleMessage: applySyncMsg } = useSyncEngine(player, rtt, () => {
  addEvent('Desync detected — correcting…', 'alert')
  playAlert()
})

// Drift correction: compare against the room clock and hard-seek when too far off
const driftTimer = setInterval(() => {
  if (!playerReady || buffering.value || !clock.isPlaying()) return
  if (!player.isPlaying.value || player.audio.seeking) return
  applySyncMsg({ type: 'sync', position: clock.current() })
}, DRIFT_CHECK_MS)
onUnmounted(() => clearInterval(driftTimer))

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
      item_id: route.query.itemId,  // the server looks up title/author from ABS
      settings: loadRoomSettings(),
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
    // Any ready-check we were part of is gone; an active one is re-sent right after room_state
    cancelBuffering()
    clock.set({ position: msg.position, playing: msg.playing, speed: msg.speed })
    participants.value = msg.participants
    Object.assign(roomSettings, normalizeRoomSettings(msg.settings))

    const me = participants.value.find(p => p.name === displayName.value)
    isHost.value = me?.is_host ?? false
    // A host whose room was lost (server restart) and just got recreated publishes its live
    // player position: fresher than the ABS progress the server started the room from.
    // Rejoining a room that still exists follows the room like everyone else.
    const publishOwnPosition = isHostMode && isHost.value && msg.participants.length === 1

    // Now open the player (saves bookmark, creates ABS session)
    if (preparing.value && auth.token && roomState.item?.id) {
      preparing.value = false
      try {
        await player.open(auth.token, roomState.item.id)
        bookmarkSaved.value = true
        setTimeout(() => { bookmarkSaved.value = false }, 5000)
        await loadBookmarks()
        playerReady = true
        // A new room already starts at the host's ABS progress (the server looks it up)
        followRoom()
      } catch (e) {
        fatalError.value = 'Failed to open playback session: ' + e.message
      }
    } else {
      // Reconnect: player already open — resync state
      if (publishOwnPosition) {
        const pos = player.currentTime.value
        clock.set({ position: pos })
        send({ type: 'seek', position: pos })
      } else if (playerReady) {
        followRoom()
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
  if (msg.type === 'settings_changed') {
    const before = { ...roomSettings }
    Object.assign(roomSettings, normalizeRoomSettings(msg.settings))
    if (before.host_only_controls !== roomSettings.host_only_controls) {
      addEvent(roomSettings.host_only_controls
        ? 'Host locked playback controls'
        : 'Everyone can control playback again', 'normal')
    }
    if (before.pause_on_join_leave !== roomSettings.pause_on_join_leave) {
      addEvent(roomSettings.pause_on_join_leave
        ? 'Playback will pause when someone joins or leaves'
        : 'Playback keeps going when someone joins or leaves', 'normal')
    }
    return
  }
  if (msg.type === 'control_denied') {
    // Our local seek/pause was rejected — snap back to the room
    addEvent('Only the host can control playback', 'alert')
    cancelBuffering()
    clock.set({ position: msg.position, playing: msg.playing })
    if (playerReady) followRoom()
    return
  }
  if (msg.type === 'error') {
    fatalError.value = msg.message
    return
  }

  if (msg.type === 'play_intent') {
    if (msg.joined) addEvent(`${msg.sender_name} joined — pausing until everyone has loaded…`, 'normal')
    else addEvent(`${msg.sender_name || 'Someone'} starting playback — buffering…`, 'normal')
    cancelBuffering()
    buffering.value = true
    clock.set({ position: msg.position, playing: false })
    // Still opening our session? The open flow calls startReadyWait() once it's done,
    // so we don't claim ready before we've loaded anything.
    if (playerReady) startReadyWait()
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

  if (msg.type === 'play') {
    // The message took ~half an RTT to get here
    clock.set({ position: msg.position + (rtt.value || 0) / 2000, playing: true })
  } else if (msg.type === 'pause') {
    clock.set({ position: msg.position, playing: false })
  } else if (msg.type === 'seek') {
    clock.set({ position: msg.position })
  } else if (msg.type === 'speed') {
    clock.set({ speed: msg.rate })
  }

  if (playerReady) applySyncMsg(msg)
}

// Bring the local player in line with the room clock (after open, reconnect, denial)
function followRoom() {
  if (buffering.value) {
    startReadyWait()
    return
  }
  player.seekTo(clock.current())
  if (clock.isPlaying()) player.play()
  else player.pause()
}

// Part of a ready-check: pause, seek to the agreed spot, report ready once buffered
function startReadyWait() {
  cancelPlayable?.()
  clearTimeout(bufferingTimeout)
  player.pause()
  player.seekTo(clock.current())
  cancelPlayable = whenPlayable(player.audio, sendReady)
  bufferingTimeout = setTimeout(sendReady, READY_FALLBACK_MS)
}

function sendReady() {
  cancelBuffering()
  send({ type: 'ready' })
}

function cancelBuffering() {
  buffering.value = false
  clearTimeout(bufferingTimeout)
  bufferingTimeout = null
  cancelPlayable?.()
  cancelPlayable = null
}

// Autoplay was refused: this click is the user gesture the browser wants
function resumeBlockedAudio() {
  player.seekTo(clock.current())
  player.play()
}

function rejoinRoom() {
  if (isHostMode && roomState.item?.id) {
    send({
      type: 'create_room',
      room_id: roomId.value,
      abs_token: auth.token,
      item_id: roomState.item.id,
      settings: { ...roomSettings },
    })
  } else if (roomId.value) {
    send({ type: 'join', room_id: roomId.value, abs_token: auth.token })
  }
}

function setKeepAwake(on) {
  keepAwake.value = on
  saveKeepAwake(on)
}

function closeMobileHint() {
  showMobileHint.value = false
  if (hintDontShowAgain.value) dismissMobileHint()
  // This click is a user gesture: a good moment for what browsers refuse without one
  wakeLock.acquire()
  if (player.blocked.value) resumeBlockedAudio()
}

function updateSetting(key, value) {
  if (!isHost.value) return
  const settings = { ...roomSettings, [key]: value }
  // Remember for the next room this user creates
  saveRoomSettings(settings)
  send({ type: 'update_settings', settings })
}

function togglePlay() {
  if (player.blocked.value) {
    // Room is playing, only our browser refused: just join in locally
    resumeBlockedAudio()
    return
  }
  if (!canControl.value) return
  const pos = player.currentTime.value
  if (player.isPlaying.value) {
    player.pause()
    clock.set({ position: pos, playing: false })
    send({ type: 'pause', position: pos })
    addEvent('You paused playback', 'normal')
  } else {
    send({ type: 'play_intent', position: pos })
    addEvent('You started playback — waiting for all to buffer…', 'normal')
  }
}

function onSeek(evt) {
  seekRoom(Number(evt.target.value))
}

function skip(seconds) {
  seekRoom(Math.max(0, player.currentTime.value + seconds))
}

function seekToBookmark(b) {
  seekRoom(b.time)
}

function seekRoom(pos) {
  if (!canControl.value) return
  player.seekTo(pos)
  clock.set({ position: pos })
  send({ type: 'seek', position: pos })
  addEvent(`You seeked to ${fmt(pos)}`, 'normal')
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
.locked-notice { font-size: 0.8rem; color: #888; }
.btn-unblock { background: #fdcb6e; color: #2d3436; border: none; border-radius: 8px; padding: 0.6rem 1rem; cursor: pointer; font-weight: 600; }
.buffering-notice { font-size: 0.8rem; color: #fdcb6e; }

.volume-row { display: flex; align-items: center; justify-content: center; }
.volume-label { display: flex; align-items: center; gap: 0.5rem; font-size: 0.85rem; color: #aaa; white-space: nowrap; }
.volume-slider { width: 90px; accent-color: #6c5ce7; cursor: pointer; }

.sidebar h3 { margin: 0 0 0.75rem; font-size: 0.9rem; color: #aaa; }
.room-settings { display: flex; flex-direction: column; gap: 0.45rem; }
.setting { display: flex; align-items: center; gap: 0.45rem; font-size: 0.8rem; color: #ccc; cursor: pointer; }
.setting input { accent-color: #6c5ce7; margin: 0; }
.setting.readonly { color: #777; cursor: default; }
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

.hint-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; padding: 1rem; z-index: 100; }
.hint-card { background: #16213e; border-radius: 12px; padding: 1.5rem; width: 100%; max-width: 380px; display: flex; flex-direction: column; gap: 0.75rem; }
.hint-card h3 { margin: 0; color: #e0e0e0; }
.hint-card p { margin: 0; font-size: 0.9rem; color: #ccc; line-height: 1.4; }
.hint-card .setting { font-size: 0.9rem; }
.hint-ok { padding: 0.65rem; background: #6c5ce7; border: none; border-radius: 6px; color: #fff; font-size: 1rem; cursor: pointer; }

/* Phones: player and sidebar stack in one scrolling column; header and event log stay put */
@media (max-width: 700px) {
  .room-header { gap: 0.5rem 1rem; padding: 0.6rem 1rem; }
  .room-meta { flex-basis: 100%; min-width: 0; }
  .room-title { font-size: 0.95rem; }
  .room-code { margin-right: auto; }
  .room-body { flex-direction: column; overflow-y: auto; }
  .player-area { flex: none; padding: 1rem; overflow: visible; }
  .player { gap: 1.1rem; }
  .cover-wrap { width: min(220px, 55vw); height: min(220px, 55vw); }
  .sidebar { width: auto; flex: none; overflow: visible; border-left: none; border-top: 1px solid #0f3460; }
  .setting, .bookmark-item, .bookmark-input, .btn-bookmark-add { font-size: 0.9rem; }
  .bookmark-item { padding: 0.25rem 0; }
  .event-log { padding: 0.35rem 1rem calc(0.35rem + env(safe-area-inset-bottom)); }
}
</style>
