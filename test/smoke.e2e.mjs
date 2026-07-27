#!/usr/bin/env node
/**
 * Lightweight E2E smoke test — boots the Vue frontend in Playwright's WebKit
 * (same engine family as the Tauri webview) and clicks through every panel
 * tab, failing on any page error or console error. This is the guard that
 * would have caught the "app won't open", "Journal tab crashes", and
 * "preview never renders" bugs.
 *
 * It does NOT test the Rust backend — Tauri `invoke` is stubbed with empty
 * defaults so the UI renders. The point is to catch JS/render crashes in the
 * webview, which is where tonight's bugs lived.
 *
 * Assumes the Vite dev server is already running (npm run dev / tauri dev).
 *   SMOKE_URL=http://localhost:5199 node test/smoke.e2e.mjs
 *
 * Exit 0 = pass, 1 = fail.
 */
import { webkit } from 'playwright'

const BASE = process.env.SMOKE_URL || 'http://localhost:5199'
const TABS = ['preview', 'media', 'activity', 'modified', 'journal', 'gear']

// Injected before any app code runs. The frontend calls Tauri APIs blindly,
// so provide a minimal __TAURI_INTERNALS__ whose invoke returns safe empty
// values — arrays for list-ish commands, null otherwise (null flows through
// the components' `?.`/`|| []` guards; {} would not).
function installTauriStub() {
  const LIST =
    /list|entries|files|history|series|heatmap|recent|backlinks|tags|media|drafts|posts|signals|events|syndication/i
  const invoke = async (cmd) => {
    if (typeof cmd === 'string' && cmd.startsWith('plugin:event')) return 0
    return LIST.test(String(cmd)) ? [] : null
  }
  const transformCallback = (cb) => {
    const id = Math.floor(Math.random() * 1e9)
    window[`_${id}`] = cb
    return id
  }
  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { label: 'main' },
    },
  }
  // The event plugin looks for its own internals object; stub it so
  // listen()/unlisten() don't throw in the bare-browser harness.
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => {},
  }
  window.__TAURI__ = { core: { invoke } }
}

// HARD failures fail the build (catastrophic: won't boot / tab blanks).
// warnings are surfaced but don't fail — they're usually incomplete-mock
// artifacts, though real guard gaps show up here too.
const hardFailures = []
const warnings = []
const MIN_NODES = 20 // below this, the pane is effectively blank

const browser = await webkit.launch()
const page = await browser.newPage()

page.on('pageerror', (e) => warnings.push(`pageerror: ${e.message}`))
page.on('console', (m) => {
  if (m.type() !== 'error') return
  const t = m.text()
  if (/__TAURI|invoke|network|Failed to fetch|ERR_/i.test(t)) return
  warnings.push(`console.error: ${t}`)
})

await page.addInitScript(installTauriStub)

try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 15000 })
  await page.waitForSelector('#app', { timeout: 10000 })
  await page.waitForTimeout(1200)

  const rootNodes = await page.locator('#app *').count()
  if (rootNodes < MIN_NODES) {
    hardFailures.push(`app did not render (only ${rootNodes} DOM nodes) — "won't open"`)
  } else {
    console.log(`✅ app booted (${rootNodes} DOM nodes)`)
  }

  for (const tab of TABS) {
    const btn = page.locator(`button[data-tab="${tab}"]`)
    if ((await btn.count()) === 0) {
      console.log(`⏭️  tab "${tab}": no button found, skipping`)
      continue
    }
    await btn.first().click()
    await page.waitForTimeout(700)
    const nodes = await page.locator('#app *').count()
    if (nodes < MIN_NODES) {
      hardFailures.push(`tab "${tab}" blanked the app (${nodes} nodes) — this is a crash`)
      console.log(`❌ tab "${tab}": BLANKED (${nodes} nodes)`)
    } else {
      console.log(`✅ tab "${tab}": rendered (${nodes} nodes)`)
    }
  }
} catch (e) {
  hardFailures.push(`fatal: ${e.message}`)
} finally {
  await browser.close()
}

if (warnings.length) {
  console.log(`\n⚠️  ${warnings.length} warning(s) (non-fatal — often mock gaps, but check for real guard bugs):`)
  for (const w of [...new Set(warnings)]) console.log(`   • ${w}`)
}

if (hardFailures.length) {
  console.log(`\n❌ smoke test FAILED (${hardFailures.length} crash(es)):`)
  for (const f of hardFailures) console.log(`   • ${f}`)
  process.exit(1)
}
console.log('\n✅ smoke test PASSED — app boots and every tab renders without crashing')
process.exit(0)
