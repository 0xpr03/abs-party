<template>
  <div class="page">
    <div class="picker">
      <header class="picker-header">
        <h2>Pick an audiobook</h2>
        <button class="btn-ghost" @click="router.back()">← Back</button>
      </header>

      <div v-if="loading" class="status">Loading libraries…</div>
      <div v-else-if="error" class="error">{{ error }}</div>

      <template v-else>
        <div class="lib-tabs">
          <button
            v-for="lib in libraries"
            :key="lib.id"
            :class="['lib-tab', { active: selectedLibId === lib.id }]"
            @click="selectLib(lib.id)"
          >{{ lib.name }}</button>
        </div>

        <div class="toolbar">
          <input
            v-model="search"
            class="search"
            placeholder="Filter by title or author…"
          />
          <select v-model="sort" class="sort-select" @change="onSortChange">
            <option value="progress">Last listened</option>
            <option value="media.metadata.title">Title</option>
            <option value="addedAt">Date added</option>
          </select>
        </div>

        <div class="grid">
          <div
            v-for="item in filteredItems"
            :key="item.id"
            class="book-card"
            @click="pick(item)"
          >
            <img
              v-if="item.media?.coverPath"
              :src="`${absBase}/api/items/${item.id}/cover`"
              :alt="item.media.metadata.title"
              class="cover"
            />
            <div v-else class="cover cover-placeholder">🎵</div>
            <div class="book-info">
              <div class="book-title">{{ item.media?.metadata?.title || 'Unknown' }}</div>
              <div class="book-author">{{ item.media?.metadata?.authorName || '' }}</div>
            </div>
          </div>
        </div>

        <div v-if="totalPages > 1 && !search" class="pagination">
          <button :disabled="page === 0" @click="changePage(page - 1)">‹</button>
          <span>{{ page + 1 }} / {{ totalPages }}</span>
          <button :disabled="page >= totalPages - 1" @click="changePage(page + 1)">›</button>
        </div>
      </template>

      <div class="room-name-overlay" v-if="pickedItem">
        <div class="overlay-card">
          <h3>Create room for "{{ pickedItem.media?.metadata?.title }}"?</h3>
          <p v-if="createError" class="error">{{ createError }}</p>
          <div class="overlay-actions">
            <button @click="pickedItem = null">Cancel</button>
            <button class="primary" @click="createRoom">Create Room</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, inject, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { getLibraries, getLibraryItems } from '../api/abs.js'

const router = useRouter()
const auth = inject('auth')
const absBase = inject('ABS_BASE')

const libraries = ref([])
const selectedLibId = ref('')
const items = ref([])
const loading = ref(true)
const error = ref('')
const search = ref('')
const sort = ref('progress')
const page = ref(0)
const totalItems = ref(0)
const PAGE_SIZE = 40

const totalPages = computed(() => Math.ceil(totalItems.value / PAGE_SIZE))

const filteredItems = computed(() => {
  if (!search.value) return items.value
  const q = search.value.toLowerCase()
  return items.value.filter(i => {
    const title = i.media?.metadata?.title?.toLowerCase() || ''
    const author = i.media?.metadata?.authorName?.toLowerCase() || ''
    return title.includes(q) || author.includes(q)
  })
})

const pickedItem = ref(null)
const createError = ref('')

onMounted(async () => {
  try {
    const data = await getLibraries(absBase, auth.token)
    libraries.value = data.libraries || []
    if (libraries.value.length > 0) {
      await selectLib(libraries.value[0].id)
    }
  } catch (e) {
    error.value = 'Failed to load libraries. Are you still signed in?'
  } finally {
    loading.value = false
  }
})

async function selectLib(libId) {
  selectedLibId.value = libId
  page.value = 0
  await loadItems()
}

async function loadItems() {
  loading.value = true
  try {
    const desc = sort.value !== 'media.metadata.title'
    const data = await getLibraryItems(absBase, auth.token, selectedLibId.value, page.value, sort.value, desc)
    items.value = data.results || []
    totalItems.value = data.total || 0
  } catch (e) {
    error.value = 'Failed to load items.'
  } finally {
    loading.value = false
  }
}

async function changePage(p) {
  page.value = p
  await loadItems()
}

async function onSortChange() {
  page.value = 0
  await loadItems()
}

function pick(item) {
  pickedItem.value = item
  createError.value = ''
}

function createRoom() {
  const item = pickedItem.value
  const meta = item.media?.metadata || {}
  router.push({
    path: '/room/new',
    query: {
      itemId: item.id,
      libraryId: selectedLibId.value,
      title: meta.title || 'Untitled',
      author: meta.authorName || '',
    },
  })
}
</script>

<style scoped>
.page { padding: 1.5rem; }
.picker { max-width: 900px; margin: 0 auto; }
.picker-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1rem; }
h2 { margin: 0; }
.btn-ghost { background: none; border: 1px solid #444; color: #ccc; padding: 0.4rem 0.9rem; border-radius: 6px; cursor: pointer; }
.lib-tabs { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.75rem; }
.lib-tab { background: #16213e; border: 1px solid #0f3460; color: #ccc; padding: 0.35rem 0.9rem; border-radius: 20px; cursor: pointer; font-size: 0.85rem; }
.lib-tab.active { background: #6c5ce7; border-color: #6c5ce7; color: #fff; }
.toolbar { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
.search { flex: 1; padding: 0.6rem 0.75rem; border: 1px solid #0f3460; border-radius: 6px; background: #16213e; color: #fff; font-size: 1rem; }
.sort-select { padding: 0.6rem 0.75rem; border: 1px solid #0f3460; border-radius: 6px; background: #16213e; color: #ccc; font-size: 0.85rem; cursor: pointer; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 1rem; }
.book-card { background: #16213e; border-radius: 8px; overflow: hidden; cursor: pointer; transition: transform 0.15s; }
.book-card:hover { transform: translateY(-3px); }
.cover { width: 100%; aspect-ratio: 1; object-fit: cover; background: #0f3460; }
.cover-placeholder { width: 100%; aspect-ratio: 1; background: #0f3460; display: flex; align-items: center; justify-content: center; font-size: 2.5rem; }
.book-info { padding: 0.5rem; }
.book-title { font-size: 0.8rem; font-weight: 600; color: #e0e0e0; line-height: 1.3; }
.book-author { font-size: 0.75rem; color: #888; margin-top: 0.2rem; }
.pagination { display: flex; align-items: center; gap: 1rem; justify-content: center; margin-top: 1.5rem; }
.pagination button { background: #16213e; border: 1px solid #0f3460; color: #ccc; padding: 0.4rem 0.9rem; border-radius: 6px; cursor: pointer; }
.pagination button:disabled { opacity: 0.4; cursor: default; }
.status { text-align: center; color: #888; padding: 3rem; }
.error { color: #ff7675; }
.room-name-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; z-index: 100; }
.overlay-card { background: #16213e; border-radius: 12px; padding: 2rem; width: 100%; max-width: 380px; }
.overlay-card h3 { margin: 0 0 1rem; }
.overlay-card label { display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.85rem; color: #aaa; margin-bottom: 1rem; }
.overlay-card input { padding: 0.6rem 0.75rem; border: 1px solid #0f3460; border-radius: 6px; background: #0f3460; color: #fff; font-size: 1rem; }
.overlay-actions { display: flex; gap: 0.75rem; }
.overlay-actions button { flex: 1; padding: 0.65rem; border: none; border-radius: 6px; cursor: pointer; background: #2d3436; color: #ccc; }
.overlay-actions .primary { background: #6c5ce7; color: #fff; }
</style>
