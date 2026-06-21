import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import Home from './views/Home.vue'
import Room from './views/Room.vue'
import BookPicker from './views/BookPicker.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: Home },
    { path: '/pick', component: BookPicker },
    { path: '/room/:id', component: Room },
  ],
})

// Fetch runtime config before mounting so ABS_BASE_URL isn't baked into the
// build artifact — change it in the server env without rebuilding the image.
// Wrapped in an IIFE to avoid top-level await (not supported by ES2020 target).
;(async () => {
  let absBaseUrl = ''
  try {
    const cfg = await fetch('/api/config').then((r) => r.json())
    absBaseUrl = cfg.abs_base_url || ''
  } catch {
    console.warn('Could not fetch /api/config — ABS base URL will be empty')
  }

  createApp(App, { absBaseUrl }).use(router).mount('#app')
})()
