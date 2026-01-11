import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

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
  server: {
    port: 3000,
    proxy: {
      // Proxy API requests to backend during development
      '/api': {
        target: 'http://localhost:9090',
        changeOrigin: true,
      },
    },
  },
})
