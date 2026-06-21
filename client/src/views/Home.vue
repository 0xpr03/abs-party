<template>
  <div class="page">
    <div class="card">
      <h1>ABS Party</h1>
      <p class="sub">Listen to audiobooks together, in sync.</p>

      <template v-if="!auth.token">
        <h2>Sign in</h2>

        <form @submit.prevent="doLogin">
          <label>Username
            <input v-model="form.username" required autocomplete="username" />
          </label>
          <label>Password
            <input v-model="form.password" type="password" required autocomplete="current-password" />
          </label>
          <p v-if="error" class="error">{{ error }}</p>
          <button type="submit" :disabled="loading">{{ loading ? 'Signing in…' : 'Sign in' }}</button>
        </form>
      </template>

      <template v-else>
        <div class="actions">
          <button @click="goCreate">Create a room</button>
        </div>
        <hr />
        <h2>Join a room</h2>
        <form @submit.prevent="doJoin">
          <label>Room code
            <input v-model="joinCode" placeholder="e.g. a1b2c3d4" required />
          </label>
          <p v-if="joinError" class="error">{{ joinError }}</p>
          <button type="submit">Join</button>
        </form>
      </template>
    </div>
  </div>
</template>

<script setup>
import { inject, ref } from 'vue'
import { useRouter } from 'vue-router'
import { login } from '../api/abs.js'

const router = useRouter()
const auth = inject('auth')
const setAuth = inject('setAuth')

const form = ref({ username: '', password: '' })
const loading = ref(false)
const error = ref('')

const joinCode = ref('')
const joinError = ref('')

async function doLogin() {
  error.value = ''
  loading.value = true
  try {
    const data = await login(form.value.username, form.value.password)
    setAuth(data.user.token, data.user.username, data.user.id)
  } catch (e) {
    error.value = 'Login failed. Check your credentials.'
  } finally {
    loading.value = false
  }
}

function goCreate() {
  router.push('/pick')
}

function doJoin() {
  if (!joinCode.value.trim()) return
  router.push(`/room/${joinCode.value.trim()}`)
}
</script>

<style scoped>
.page { display: flex; justify-content: center; padding: 3rem 1rem; }
.card { background: #16213e; border-radius: 12px; padding: 2rem; width: 100%; max-width: 420px; }
h1 { margin: 0 0 0.25rem; color: #a29bfe; }
.sub { margin: 0 0 1.5rem; color: #888; }
h2 { margin: 1rem 0 0.75rem; font-size: 1rem; color: #ccc; }
form { display: flex; flex-direction: column; gap: 0.75rem; }
label { display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.85rem; color: #aaa; }
input { padding: 0.6rem 0.75rem; border: 1px solid #0f3460; border-radius: 6px; background: #0f3460; color: #fff; font-size: 1rem; }
button[type=submit] { padding: 0.7rem; background: #6c5ce7; border: none; border-radius: 6px; color: #fff; font-size: 1rem; cursor: pointer; }
button[type=submit]:disabled { opacity: 0.6; cursor: default; }
.error { color: #ff7675; font-size: 0.85rem; margin: 0; }
.actions button { width: 100%; padding: 0.7rem; background: #6c5ce7; border: none; border-radius: 6px; color: #fff; font-size: 1rem; cursor: pointer; }
hr { border: none; border-top: 1px solid #0f3460; margin: 1.5rem 0; }
</style>
