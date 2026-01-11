import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  define: {
    // Version is set via VITE_APP_VERSION env var during release workflow
    // Falls back to 'dev' for local development
    __APP_VERSION__: JSON.stringify(process.env.VITE_APP_VERSION || 'dev'),
    __BUILD_TIME__: JSON.stringify(new Date().toISOString()),
  },
  css: {
    postcss: './postcss.config.js',
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 3001,
    proxy: {
      // Proxy API requests to exit node backend during development
      '/api': {
        target: 'http://localhost:3080',
        changeOrigin: true,
      },
    },
  },
})
