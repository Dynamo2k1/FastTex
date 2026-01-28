import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: Number(process.env.VITE_DEV_PORT) || 5173,
    proxy: {
      '/sync': {
        target: process.env.VITE_WS_URL || 'ws://localhost:8080',
        ws: true,
      },
      '/compile': {
        target: process.env.VITE_WS_URL || 'ws://localhost:8080',
        ws: true,
      },
      '/api': {
        target: process.env.VITE_API_URL || 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'yjs',
      'y-websocket',
      'y-indexeddb',
      'y-codemirror.next',
      '@codemirror/state',
      '@codemirror/view',
      '@codemirror/commands',
      '@codemirror/language',
      '@codemirror/autocomplete',
      '@codemirror/search',
      'codemirror',
    ],
  },
})
