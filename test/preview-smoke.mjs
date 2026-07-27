#!/usr/bin/env node
/**
 * Preview pipeline smoke test — the cheap guard that would have caught the
 * "previews never render" class of bug (a moved preview-convert.mjs script,
 * a broken website2 pipeline, a dead preview server).
 *
 * It boots the REAL preview server, feeds it a fixture markdown file, and
 * asserts it renders non-empty HTML within a timeout. No browser, no vault —
 * pure Node, safe to run in CI.
 *
 *   node test/preview-smoke.mjs            # uses $WEBSITE2_PATH or default
 *
 * Exit 0 = pass, 1 = fail, 2 = skipped (website2 pipeline not available).
 */
import { spawn } from 'child_process'
import { mkdtempSync, writeFileSync, existsSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { fileURLToPath } from 'url'

const ROOT = join(fileURLToPath(import.meta.url), '..', '..')
const PORT = 6419
const WEBSITE2_PATH = process.env.WEBSITE2_PATH || '/Users/ejfox/code/website2'

const CONVERT_CANDIDATES = [
  'scripts/author/preview-convert.mjs',
  'scripts/preview-convert.mjs',
]

function log(ok, msg) {
  console.log(`${ok ? '✅' : '❌'} ${msg}`)
}

async function main() {
  // Pre-flight: the pipeline this test exercises lives in website2. If it's
  // not checked out, skip loudly rather than fail — CI without website2 can't
  // run this.
  if (!existsSync(WEBSITE2_PATH)) {
    console.log(`⏭️  SKIP: website2 not found at ${WEBSITE2_PATH}`)
    process.exit(2)
  }
  const convert = CONVERT_CANDIDATES.find((c) => existsSync(join(WEBSITE2_PATH, c)))
  if (!convert) {
    // THIS is the exact failure we hit tonight — surface it as a hard fail.
    log(false, `preview-convert.mjs missing in ${WEBSITE2_PATH}`)
    console.log(`   looked in: ${CONVERT_CANDIDATES.join(', ')}`)
    process.exit(1)
  }
  log(true, `found convert script: ${convert}`)

  // A fixture note that exercises headings, prose, a blockquote and a
  // wikilink — the shapes that broke before.
  const dir = mkdtempSync(join(tmpdir(), 'dispatch-smoke-'))
  const fixture = join(dir, 'smoke.md')
  writeFileSync(
    fixture,
    `# Smoke Test\n\nA paragraph with a [[wikilink]] and some **bold** text.\n\n> A blockquote.\n\n- one\n- two\n`,
  )

  const server = spawn(process.execPath, [join(ROOT, 'preview-server.mjs')], {
    env: { ...process.env, WEBSITE2_PATH },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const kill = () => {
    try { server.kill('SIGKILL') } catch {}
  }
  process.on('exit', kill)

  const base = `http://127.0.0.1:${PORT}`
  const deadline = Date.now() + 20000

  try {
    // Wait for the server to accept connections.
    while (Date.now() < deadline) {
      try {
        await fetch(base + '/content')
        break
      } catch {
        await new Promise((r) => setTimeout(r, 200))
      }
    }

    // Prime it with the fixture, then poll /content until the render lands.
    await fetch(base + '/set-file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: fixture }),
    })

    let html = ''
    while (Date.now() < deadline) {
      const data = await (await fetch(base + '/content')).json()
      if (data.html && data.html.trim()) { html = data.html; break }
      await new Promise((r) => setTimeout(r, 200))
    }

    if (!html) {
      log(false, 'preview server produced NO html (render failed or timed out)')
      process.exit(1)
    }
    if (!html.includes('Smoke Test')) {
      log(false, `rendered html missing expected content. Got: ${html.slice(0, 120)}`)
      process.exit(1)
    }
    log(true, `rendered ${html.length} chars of HTML from the fixture`)
    console.log('\n✅ preview smoke test PASSED')
    process.exit(0)
  } finally {
    kill()
  }
}

main().catch((e) => {
  log(false, `unexpected error: ${e.message}`)
  process.exit(1)
})
