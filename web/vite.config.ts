import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      manifest: {
        name: 'CBPai',
        short_name: 'CBPai',
        start_url: '/',
        display: 'standalone',
        background_color: '#0e0f13',
        theme_color: '#0e0f13',
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
