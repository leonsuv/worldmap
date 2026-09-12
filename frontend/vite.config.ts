import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'

export default defineConfig({
  plugins: [
    react(),
    visualizer({ filename: 'dist/bundle-stats.html', gzipSize: true, brotliSize: true }),
  ],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        ws: true,
      },
      '/tiles': 'http://localhost:3000',
    },
  },
  build: {
    sourcemap: false,
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          // Shared bundler helpers must not make the UI eagerly load the map engine.
          if (id.includes('commonjsHelpers') || id.includes('vite/preload-helper')) return 'runtime'
          if (id.includes('/node_modules/maplibre-gl/')) return 'maplibre'
          if (/\/node_modules\/@(?:deck|luma|loaders|math)\.gl\//.test(id)) return 'deckgl'
        },
      },
    },
  },
})
