/**
 * DEV-ONLY Tauri mock for looking at the UI in a plain browser.
 *
 * Loaded by main.ts only when `import.meta.env.DEV` AND the page URL has
 * `?mock`. Vite replaces `import.meta.env.DEV` with `false` in production
 * builds, so the dynamic import that pulls this file in is dead code there
 * and the module is never emitted. Nothing here ships.
 *
 * It installs `window.__TAURI_INTERNALS__` with an `invoke` that answers the
 * commands FilePreview and App.vue call with fixture data shaped like the
 * Rust return types (src-tauri/src/lib.rs `MarkdownFile`, obsidian.rs
 * `Backlink`, cloudinary.rs `LocalMediaRef`, …), plus a tiny event bus so
 * `listen()` works and the mock can fire a menu event after mount.
 *
 *   ?mock            install the mock, show the first fixture in Preview
 *   ?mock&file=N     show fixture N (0 live photo post · 1 not published ·
 *                    2 live, changed since live · 3 scheduled)
 *   ?mock&overlay    at a narrow width, also open the inspector overlay
 *
 * Publishing (nothing leaves the browser): the review sheet reads a fake
 * `publish_preview`, and confirming it plays a fake deploy.
 *   &deploy=live|failed|timeout|unverified|hidden   how the deploy ends
 *                                                   (default live)
 *   &push=fail       publish_file fails the way a rejected push does
 *   &embargo         the post has a future publishAt (sheet says Schedule)
 */
import type { MarkdownFile, Backlink, LocalMediaRef, AppConfig, GitStatus, PostAnalytics } from '../types'

const params = new URLSearchParams(location.search)
const VAULT = '/Users/ej/vault'

// A self-contained gray image so the gallery renders without the network.
function fakeImage(n: number, w = 640, h = 480): string {
  // Double quotes only: the gallery lifts `src` out of the rendered HTML, so
  // an entity-escaped quote would end up in the URL.
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="hsl(${(n * 37) % 360} 12% ${28 + (n % 4) * 6}%)"/><text x="50%" y="52%" font-family="Helvetica" font-size="${Math.round(h / 6)}" fill="rgba(255,255,255,0.35)" text-anchor="middle">${n}</text></svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

const photoBody =
  `Scans from the first two years I carried a camera everywhere.\n\n` +
  Array.from({ length: 18 }, (_, i) => `![Roll ${Math.floor(i / 6) + 1}, frame ${(i % 6) + 1}](${fakeImage(i + 1)})`).join(
    '\n\n',
  ) +
  `\n\nMost of these are from the 2009 trip; the last six are from the following spring.\n`

const essayBody = `A field guide to the small decisions that make a chart feel finished — and why the last ten percent of a visualization takes half the time.

## Start with the question

Every chart answers a question. If you can't say what the question is in one sentence, the chart isn't ready, no matter how good the axis labels look.

- What does the reader already believe?
- What would change their mind?
- What is the smallest honest comparison that does that?

## Remove, then remove again

The grid is a scaffold. Once the bars stand on their own, take it down. The same goes for legends: label the lines directly and the legend has nothing left to do.

> The data-ink ratio is not a rule, it's a diagnostic. When the ratio is low you have a *reason* to look again.

\`\`\`js
const ticks = scale.ticks(4) // four is almost always enough
\`\`\`

## Let the type do the work

Numbers set in tabular figures line up; prose set in a serif reads as prose. Mixing the two on purpose is what makes a dashboard feel like a page instead of a form.
`

const now = Math.floor(Date.now() / 1000)
const day = 86400

const files: MarkdownFile[] = [
  {
    path: `${VAULT}/blog/2010/photography-2008-2010.md`,
    filename: 'photography-2008-2010.md',
    title: 'Photography 2008-2010',
    dek: null,
    date: '2010-01-01',
    tags: ['photography', 'film', 'archive'],
    created: now - 5800 * day,
    modified: now - 40 * day,
    word_count: 57,
    published_word_count: 57,
    is_safe: true,
    warnings: [],
    published_url: 'https://ejfox.com/blog/photography-2008-2010',
    published_date: now - 39 * day,
    source_dir: 'blog/2010',
    unlisted: false,
    password: null,
    publish_at: null,
    content_type: 'photos',
    image_count: 18,
    thumbnail: fakeImage(1),
  },
  {
    path: `${VAULT}/blog/2026/finishing-a-chart.md`,
    filename: 'finishing-a-chart.md',
    title: 'Finishing a chart',
    dek: 'The last ten percent of a visualization takes half the time',
    date: '2026-10-09',
    tags: ['dataviz', 'process'],
    created: now - 6 * day,
    modified: now - 3600,
    word_count: 1240,
    published_word_count: null,
    is_safe: false,
    warnings: ['Missing alt text (2)', 'Has TODO markers', '[privacy] Mentions a client by name'],
    published_url: null,
    published_date: null,
    source_dir: 'blog/2026',
    unlisted: false,
    password: null,
    publish_at: null,
    content_type: 'blog',
    image_count: 2,
    thumbnail: null,
    draft: false,
  },
  {
    path: `${VAULT}/blog/2026/the-boring-infra-essay.md`,
    filename: 'the-boring-infra-essay.md',
    title: 'Boring infrastructure is a feature',
    dek: null,
    date: '2026-09-21',
    tags: ['infra', 'essays'],
    created: now - 30 * day,
    modified: now - 2 * 3600,
    word_count: 2210,
    published_word_count: 2104,
    is_safe: true,
    warnings: ['Modified since publish'],
    published_url: 'https://ejfox.com/blog/the-boring-infra-essay',
    published_date: now - 20 * day,
    source_dir: 'blog/2026',
    unlisted: false,
    password: null,
    publish_at: null,
    content_type: 'blog',
    image_count: 0,
    thumbnail: null,
  },
  {
    path: `${VAULT}/blog/2026/next-week.md`,
    filename: 'next-week.md',
    title: 'Notes for next week',
    dek: null,
    date: '2026-10-14',
    tags: ['week-notes'],
    created: now - day,
    modified: now - 600,
    word_count: 410,
    published_word_count: null,
    is_safe: true,
    warnings: [],
    published_url: null,
    published_date: null,
    source_dir: 'blog/2026',
    unlisted: false,
    password: null,
    publish_at: '2026-10-14T09:00:00-04:00',
    content_type: 'weeknote',
    image_count: 0,
    thumbnail: null,
  },
]

const contents: Record<string, string> = {
  [files[0].path]: `---\ntitle: Photography 2008-2010\ndate: 2010-01-01\ntype: photos\n---\n${photoBody}`,
  [files[1].path]: `---\ntitle: Finishing a chart\n---\n${essayBody}\n![](charts/final.png)\n\n![](charts/grid.png)\n`,
  [files[2].path]: `---\ntitle: Boring infrastructure is a feature\n---\n${essayBody}`,
  [files[3].path]: `---\ntitle: Notes for next week\n---\n${essayBody.slice(0, 600)}\n`,
}

const backlinks: Record<string, Backlink[]> = {
  [files[2].path]: [
    { path: `${VAULT}/blog/2026/pm2-not-docker.md`, title: 'pm2, not Docker', context: 'as I argued in [[the-boring-infra-essay]]' },
    { path: `${VAULT}/blog/2025/one-process.md`, title: 'One process', context: 'see [[the-boring-infra-essay|the essay]]' },
  ],
}

const localMedia: Record<string, LocalMediaRef[]> = {
  [files[1].path]: [
    { original_text: '![](charts/final.png)', path: 'charts/final.png', resolved_path: `${VAULT}/charts/final.png`, alt_text: null, media_type: 'image', line_number: 31 },
    { original_text: '![](charts/grid.png)', path: 'charts/grid.png', resolved_path: `${VAULT}/charts/grid.png`, alt_text: null, media_type: 'image', line_number: 33 },
  ],
}

const appConfig: AppConfig = {
  version: 3,
  vault: { path: VAULT, name: 'vault', excluded_dirs: [], publishable_dirs: ['blog'] },
  publish_targets: [
    {
      name: 'ejfox.com',
      id: 'ejfox',
      repo_path: '/Users/ej/code/website2',
      domain: 'https://ejfox.com',
      content_path_pattern: 'content/blog/{year}/{slug}.md',
      branch: 'main',
      is_default: true,
    },
  ],
  editors: [
    { name: 'Obsidian', app_name: 'Obsidian', enabled: true },
    { name: 'iA Writer', app_name: 'iA Writer', enabled: true },
  ],
  default_editor: 'iA Writer',
  cloudinary_cloud_name: 'ejf',
  analytics_url: 'https://umami.tools.ejfox.com',
  mastodon_instance: null,
  media: { primary: 'cloudinary', mirrors: [], cloudinary: null, r2: null },
  webmentions_bridgy_fed: false,
}

const gitStatus: GitStatus = { ok: true, branch: 'main', error: null, dirty_files: [], has_conflicts: false }
const analytics: PostAnalytics = { pageviews: 1284, visitors: 962, visits: 1103, bounces: 512, totaltime: 70_600 }
const series = Array.from({ length: 30 }, (_, i) => Math.round(20 + 30 * Math.abs(Math.sin(i / 3)) + (i % 5) * 4))

// ── Event bus (listen / emit) ───────────────────────────────────────────────
type Handler = (e: { event: string; id: number; payload: unknown }) => void
const callbacks = new Map<number, Handler>()
const listeners = new Map<string, Set<number>>()
let nextCb = 1
let nextEvent = 1

function emit(event: string, payload: unknown = null) {
  for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id: nextEvent++, payload })
}

// ── Publishing (fake) ───────────────────────────────────────────────────────
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const MOCK_SHA = 'd57aecc4ff85880009177738ccd95fc05d3ffa78'
function mockUrl(path: string) {
  const f = files.find((x) => x.path === path)
  return f?.published_url ?? `https://ejfox.com/blog/2026/${(f?.filename ?? 'post.md').replace(/\.md$/, '')}`
}
function mockPreview({ sourcePath }: { sourcePath: string }) {
  const f = files.find((x) => x.path === sourcePath)!
  const embargo = params.has('embargo')
    ? { key: 'publishAt', raw: '2026-10-14T09:00:00-04:00', utc: '2099-10-14T13:00:00Z', date_only: false }
    : null
  return {
    title: f.title ?? f.filename,
    slug: f.filename.replace(/\.md$/, ''),
    kind: 'blog',
    is_update: !!f.published_url,
    url: mockUrl(sourcePath),
    rel_path: `content/blog/2026/${f.filename}`,
    repo_name: 'website2',
    repo_path: '/Users/ej/code/website2',
    branch: 'main',
    current_branch: 'main',
    on_branch: true,
    other_uncommitted: 0,
    clobber_ok: true,
    visibility: 'public',
    embargo,
    dispatch_schedule: null,
    webmentions_after_live: true,
    bridgy_fed: false,
    syndication: [],
    deploy_estimate_secs: 180,
    blocked: null,
  }
}
async function mockWaitForDeploy({ sha, hiddenUntil }: { sha: string; hiddenUntil: string | null }) {
  for (const s of [10, 20, 30]) {
    await sleep(700)
    emit('deploy-progress', { sha, elapsed_secs: s, deployed_commit: 'a30dc6fb', message: `deploying ${sha.slice(0, 7)}` })
  }
  const outcome = hiddenUntil ? 'hidden' : (params.get('deploy') ?? 'live')
  const run = 'https://github.com/ejfox/website2/actions/runs/1'
  const r = (phase: string, ok: boolean, message: string) => ({
    sha, phase, ok, message, deployed_commit: 'd57aecc4', url_status: ok ? 200 : 404, run_url: run,
  })
  switch (outcome) {
    case 'failed':
      return r('failed', false, 'GitHub’s deploy run ended in failure, so the site kept the previous version.')
    case 'timeout':
      return r('timeout', false, 'After 6 min the site still runs a30dc6fb, not d57aecc. The deploy may be queued or slow.')
    case 'unverified':
      return r('deployed_unverified', false, 'The site runs your commit (d57aecc4), but the post’s URL answered HTTP 404.')
    case 'hidden':
      return r('hidden', true, `Deployed. The site keeps it hidden until ${hiddenUntil}.`)
    default:
      return r('live', true, 'Live')
  }
}

// ── invoke ──────────────────────────────────────────────────────────────────
const commands: Record<string, (args: any) => unknown> = {
  get_recent_files: () => files,
  get_file_content: ({ path }) => contents[path] ?? '',
  get_app_config: () => appConfig,
  get_git_status: () => gitStatus,
  check_obsidian_api: () => true,
  check_cloudinary_status: () => true,
  check_analytics_status: () => true,
  check_media_status: () => ({ cloudinary: true, r2: false, primary: 'cloudinary', mirrors: [] }),
  get_backlinks: ({ filename }) => backlinks[files.find((f) => f.filename === filename)?.path ?? ''] ?? [],
  get_local_media: ({ path }) => localMedia[path] ?? [],
  get_post_analytics: () => analytics,
  get_post_sources: () => [
    { x: 'bluesky', y: 412 },
    { x: 'direct', y: 301 },
    { x: 'hn', y: 188 },
    { x: 'newsletter', y: 97 },
  ],
  get_post_pageview_series: () => series,
  is_vue_page: () => false,
  publish_preview: mockPreview,
  publish_file: async ({ sourcePath }) => {
    await sleep(900)
    if (params.get('push') === 'fail')
      throw "Committed, but the push to origin/main failed, so the site hasn't changed. Trying again pushes it.\ngit push origin main failed: fatal: unable to access 'https://github.com/ejfox/website2.git/': Could not resolve host: github.com"
    return mockUrl(sourcePath)
  },
  get_head_sha: () => MOCK_SHA,
  wait_for_deploy: mockWaitForDeploy,
  get_publish_diff: ({ filePath }) => ({
    has_diff: true,
    source_path: filePath,
    published_path: '/Users/ej/code/website2/content/blog/2026/the-boring-infra-essay.md',
    words_added: 106,
    words_removed: 12,
    lines_added: 4,
    lines_removed: 1,
    hunks: [],
    error: null,
  }),
  generate_og_variants: () =>
    new Promise((_, reject) => setTimeout(() => reject('Preview server is not running (mock)'), 400)),
  // Empty Media and Journal tabs, so their empty states can be checked.
  cloudinary_list_folders: () => [],
  cloudinary_list_assets: () => ({ assets: [], next_cursor: null }),
  cloudinary_search: () => ({ assets: [], next_cursor: null }),
  scan_asset_usage: () => ({
    usage_map: { by_asset: {}, by_post: {}, total_assets: 0, total_posts: 0 },
    cloudinary_urls: [],
    scan_duration_ms: 0,
  }),
  get_journal_entries: () => [],
  get_journal_heatmap: () => [],
  backfill_journal: () => 0,
  desk_today: () => ({ date: '', day_dir: '', streak: 0, week: [], shipped_today: false, slot: { start: '', open: false }, bench: null, stories: [] }),
  'plugin:event|listen': ({ event, handler }) => {
    if (!listeners.has(event)) listeners.set(event, new Set())
    listeners.get(event)!.add(handler)
    // As soon as the app listens for the View menu, switch it to Preview
    // (the fixture is already the restored selection).
    if (event === 'menu-panel-preview') {
      setTimeout(() => emit('menu-panel-preview'), 150)
      if (params.has('overlay')) {
        setTimeout(() => document.querySelector<HTMLButtonElement>('.inspector-toggle')?.click(), 800)
      }
    }
    return handler
  },
  'plugin:event|unlisten': ({ event, eventId }) => void listeners.get(event)?.delete(eventId),
  'plugin:menu|new': () => [nextCb++, `menu-${nextCb}`],
  'plugin:menu|popup': () => null,
}

const internals = {
  metadata: {
    currentWindow: { label: 'main' },
    currentWebview: { label: 'main', windowLabel: 'main' },
    windows: [{ label: 'main' }],
    webviews: [{ label: 'main', windowLabel: 'main' }],
  },
  transformCallback(cb: Handler, _once?: boolean) {
    const id = nextCb++
    callbacks.set(id, cb)
    ;(window as any)[`_${id}`] = cb
    return id
  },
  unregisterCallback(id: number) {
    callbacks.delete(id)
    delete (window as any)[`_${id}`]
  },
  convertFileSrc(path: string) {
    return `asset://localhost/${encodeURIComponent(path)}`
  },
  async invoke(cmd: string, args: any) {
    const fn = commands[cmd]
    if (!fn) {
      // Window / dialog / clipboard plumbing and the rest: succeed quietly.
      return null
    }
    return fn(args ?? {})
  },
}

;(window as any).__TAURI_INTERNALS__ = internals
;(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener(event: string, id: number) {
    listeners.get(event)?.delete(id)
  },
}
;(window as any).__TAURI_OS_PLUGIN_INTERNALS__ = { os_type: 'macos' }

// Land on the chosen fixture in the Preview tab. App.vue restores the
// selection from `dispatch-last-file`; the tab comes from a View-menu event
// the app already listens for.
const which = Math.max(0, Math.min(files.length - 1, Number(params.get('file') ?? 0)))
localStorage.setItem('dispatch-last-file', files[which].path)

export function afterMount() {
  /* The Preview tab is selected from the `plugin:event|listen` hook above. */
}

console.info('[mock] Tauri mock installed · fixture', which, files[which].filename)
