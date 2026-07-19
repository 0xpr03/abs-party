<template>
  <div class="app-root">
    <nav class="nav">
      <span class="nav-brand">🎧 ABS Party</span>
      <span v-if="auth.username" class="nav-user">
        {{ auth.username }}
        <button class="btn-sm" @click="logout">Logout</button>
      </span>
    </nav>
    <div class="app-content">
      <router-view />
    </div>
  </div>
</template>

<script setup>
import { reactive, provide } from 'vue'
import { useRouter } from 'vue-router'

const props = defineProps({
  absBaseUrl: { type: String, default: '' },
})

const router = useRouter()

const auth = reactive({
  token: localStorage.getItem('abs_token') || '',
  username: localStorage.getItem('abs_username') || '',
  userId: localStorage.getItem('abs_user_id') || '',
})

function setAuth(token, username, userId) {
  auth.token = token
  auth.username = username
  auth.userId = userId
  localStorage.setItem('abs_token', token)
  localStorage.setItem('abs_username', username)
  localStorage.setItem('abs_user_id', userId)
}

function logout() {
  auth.token = ''
  auth.username = ''
  auth.userId = ''
  localStorage.removeItem('abs_token')
  localStorage.removeItem('abs_username')
  localStorage.removeItem('abs_user_id')
  router.push('/')
}

provide('auth', auth)
provide('setAuth', setAuth)
provide('ABS_BASE', props.absBaseUrl)
</script>

<style scoped>
.nav {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1.5rem;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
}
.app-root { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
.app-content { flex: 1; overflow: hidden; min-height: 0; }
.nav-brand { font-size: 1.2rem; font-weight: 700; color: #a29bfe; }
.nav-user { display: flex; align-items: center; gap: 0.75rem; font-size: 0.9rem; }
.btn-sm { padding: 0.25rem 0.75rem; background: #6c5ce7; border: none; border-radius: 4px; color: #fff; cursor: pointer; font-size: 0.8rem; }
</style>
