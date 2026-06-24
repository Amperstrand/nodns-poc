import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    preact({ reactAliases: true }),
    VitePWA({
      registerType: 'autoUpdate',
      injectRegister: true,
      manifest: {
        name: 'NoDNS Registrar (BETA)',
        short_name: 'NoDNS',
        description: 'Decentralized DNS records from Nostr events. Use test sats only.',
        theme_color: '#0a0a0a',
        background_color: '#0a0a0a',
        display: 'standalone',
        scope: '/',
        start_url: '/',
        icons: [
          { src: '/icon.svg', sizes: 'any', type: 'image/svg+xml' },
        ],
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,woff2}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/api\//],
      },
    }),
  ],
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  build: {
    outDir: 'dist',
    target: 'es2022',
    sourcemap: true,
  },
  server: {
    port: 5175,
    proxy: {
      '/api': {
        target: 'https://nodns.shop',
        changeOrigin: true,
      },
    },
  },
});
