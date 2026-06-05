import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      manifest: {
        name: 'Кухня',
        short_name: 'Кухня',
        start_url: '/',
        display: 'standalone',
        background_color: '#111111',
        theme_color: '#111111',
        icons: []
      }
    })
  ],
  server: {
    // dev: проксируем API на локальный Rust-бэкенд
    proxy: { '/api': 'http://localhost:3000' }
  },
  build: { outDir: 'dist' }
});
