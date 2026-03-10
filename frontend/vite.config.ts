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
        manualChunks: {
          'maplibre': ['maplibre-gl'],
          'deckgl': ['@deck.gl/core', '@deck.gl/layers', '@deck.gl/geo-layers', '@deck.gl/mapbox'],
        },
      },
    },
  },
})
