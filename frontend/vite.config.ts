/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'

const backend = process.env.WORLDMAP_BACKEND ?? 'http://127.0.0.1:3000'

export default defineConfig(({ mode }) => ({
  plugins: [react(), mode === 'analyze' && visualizer({ filename: 'dist/bundle-stats.html', gzipSize: true, brotliSize: true })],
  server: {
    proxy: {
      '/api': { target: backend, ws: true },
      '/tiles': backend,
    },
  },
  preview: {
    proxy: {
      '/api': { target: backend, ws: true },
      '/tiles': backend,
    },
  },
  build: {
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('/node_modules/maplibre-gl/')) return 'maplibre'
          if (/\/node_modules\/@(?:deck|luma|loaders|math|probe)\.gl\//.test(id)) return 'deckgl'
        },
      },
    },
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
  },
}))
