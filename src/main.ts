import { createApp, defineAsyncComponent } from 'vue'
import App from './App.vue'
import './styles/global.css'
import { invoke } from '@tauri-apps/api/core'

// Release builds have no devtools: send errors to dispatch.log instead.
if (!import.meta.env.DEV) {
  const send = (level: string, parts: unknown[]) => {
    const message = parts
      .map((p) => (p instanceof Error ? `${p.message}\n${p.stack ?? ''}` : typeof p === 'string' ? p : JSON.stringify(p)))
      .join(' ')
    invoke('log_frontend', { level, message }).catch(() => {})
  }
  const origError = console.error.bind(console)
  console.error = (...args: unknown[]) => {
    origError(...args)
    send('error', args)
  }
  window.addEventListener('error', (e) => send('error', [e.error ?? e.message]))
  window.addEventListener('unhandledrejection', (e) => send('error', ['unhandled rejection:', e.reason]))
}

// One bundle, two windows: the Settings window (src-tauri/src/settings.rs)
// loads index.html?window=settings and mounts its own root instead of App.
const isSettingsWindow = new URLSearchParams(location.search).get('window') === 'settings'

function mount() {
  const root = isSettingsWindow
    ? defineAsyncComponent(() => import('./components/settings/SettingsWindow.vue'))
    : App
  createApp(root).mount('#app')
}

// Dev-only: `?mock` in a plain browser stubs the Tauri bridge with fixture
// data so the UI can be looked at (and screenshotted) outside the app. The
// condition is a compile-time constant in production builds, so the mock
// module is never bundled. See src/dev/mockTauri.ts.
if (import.meta.env.DEV && new URLSearchParams(location.search).has('mock')) {
  import('./dev/mockTauri').then((m) => {
    mount()
    m.afterMount()
  })
} else {
  mount()
}
