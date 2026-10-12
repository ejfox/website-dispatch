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

// ── invoke ──────────────────────────────────────────────────────────────────
const commands: Record<string, (args: any) => unknown> = {
  get_recent_files: () => files,
  get_file_content: ({ path }) => contents[path] ?? '',
  get_app_config: () => appConfig,
  // Settings window (?mock&window=settings). Secrets come back masked, as
  // from settings.rs; nothing real is stored.
  get_settings: () => ({
    config: {
      ...JSON.parse(JSON.stringify(appConfig)),
      media: {
        primary: 'cloudinary',
        mirrors: [],
        cloudinary: { cloud_name: 'ejf', api_key: '\u2022\u2022\u2022\u20224821', api_secret: '\u2022\u2022\u2022\u20229f0c' },
        r2: null,
      },
    },
    config_path: '/Users/ej/Library/Application Support/com.ejfox.dispatch/config.json',
    config_exists: true,
    env: {
      cloudinary: false,
      r2: false,
      mastodon_token: true,
      mastodon_instance: null,
      bluesky: false,
      umami: true,
      umami_url: 'https://umami.tools.ejfox.com',
      obsidian_api_key: true,
      alt_text_provider: 'anthropic',
      dotenv_files: ['/Users/ej/code/website-dispatch/.env'],
    },
  }),
  save_settings: ({ configData }) => {
    const mask = (v: string) => (!v || v.startsWith('\u2022') ? v : '\u2022\u2022\u2022\u2022' + (v.length >= 12 ? v.slice(-4) : ''))
    const c = JSON.parse(JSON.stringify(configData))
    if (c.media?.cloudinary) {
      c.media.cloudinary.api_key = mask(c.media.cloudinary.api_key)
      c.media.cloudinary.api_secret = mask(c.media.cloudinary.api_secret)
    }
    if (c.media?.r2) {
      c.media.r2.access_key_id = mask(c.media.r2.access_key_id)
      c.media.r2.secret_access_key = mask(c.media.r2.secret_access_key)
    }
    console.info('[mock] save_settings')
    return c
  },
  check_vault_folder: ({ path, publishableDirs }) =>
    path.startsWith(VAULT)
      ? { exists: true, is_obsidian_vault: true, folder_name: 'vault', missing_dirs: (publishableDirs as string[]).filter((d) => d !== 'blog') }
      : { exists: false, is_obsidian_vault: false, folder_name: '', missing_dirs: [] },
  check_publish_target: ({ repoPath, branch, contentPathPattern }) => ({
    exists: repoPath.startsWith('/Users/ej/'),
    is_git: repoPath.startsWith('/Users/ej/code'),
    current_branch: 'main',
    branch_exists: branch === 'main' || branch === 'dev',
    on_branch: branch === 'main',
    has_origin: true,
    content_dir_exists: String(contentPathPattern).startsWith('content/blog'),
    content_dir: String(contentPathPattern).split('/{year}')[0],
  }),
  check_app_installed: ({ appName }) => appName !== 'Sublime Text',
  verify_mastodon: () => 'ej@mastodon.social (EJ Fox)',
  open_settings_window: ({ pane }) => void window.open(`?mock&window=settings${pane ? `&pane=${pane}` : ''}`, 'settings', 'width=640,height=560'),
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
