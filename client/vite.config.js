import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// The dev server URL for the Rust backend. Override with VITE_DEV_API_URL if
// running the backend on a non-default port.
const backendUrl = process.env.VITE_DEV_API_URL || 'http://localhost:3456'
const backendWs = backendUrl.replace(/^http/, 'ws')

export default defineConfig({
  plugins: [vue()],
  build: {
    outDir: '../server/static',
    emptyOutDir: true,
  },
  server: {
    proxy: {
      // All /api/* calls (config, abs login proxy) go to the backend
      '/api': { target: backendUrl, changeOrigin: true },
      // WebSocket upgrade
      '/ws': { target: backendWs, ws: true },
    },
  },
})
