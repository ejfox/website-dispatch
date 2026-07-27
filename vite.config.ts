import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath } from 'node:url'
import { version } from './package.json'

export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },
  resolve: {
    alias: {
      // `decode-named-character-reference` (a remark/micromark dependency)
      // ships a `browser` build (index.dom.js) that does
      // `document.createElement("i")` at module load. That works on the main
      // thread but HARD-CRASHES the markdown Web Worker with
      // "Can't find variable: document", killing preview rendering. Force the
      // DOM-free `index.js` build (the package's own `worker`/`default`
      // condition) — it's a pure lookup table and works everywhere.
      'decode-named-character-reference': fileURLToPath(
        new URL(
          './node_modules/decode-named-character-reference/index.js',
          import.meta.url,
        ),
      ),
    },
  },
  clearScreen: false,
  server: {
    port: 5199,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: ['es2021', 'chrome100', 'safari13'],
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    rollupOptions: {
      output: {
        manualChunks: {
          vendor: ['vue', '@tauri-apps/api'],
          icons: ['@phosphor-icons/vue', 'lucide-vue-next'],
          markdown: ['unified', 'remark-parse', 'remark-gfm', 'remark-rehype', 'rehype-raw', 'rehype-stringify'],
        },
      },
    },
  },
})
