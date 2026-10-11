import { createApp } from 'vue'
import App from './App.vue'
import './styles/global.css'

function mount() {
  createApp(App).mount('#app')
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
