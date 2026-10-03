# Dispatch

A Tauri desktop app for publishing Obsidian notes to your website. Keyboard-driven, configurable, fast.

<img width="900" height="1131" alt="screenshot 2026-01-26 at 11 40 14 AM" src="https://github.com/user-attachments/assets/6964af9d-aa81-4184-85d3-e0a8ef27c27f" />

## Features

- Scans Obsidian vault (`blog/` and `drafts/` folders)
- One-click publish with git commit/push
- Tracks LIVE / MODIFIED / draft status via content diff
- Keyboard-driven navigation (j/k, gg/G, 1-9, `?` for help)
- Live preview using exact website markdown pipeline
- Scheduled publishing with frontmatter `publish_at`
- Visibility controls: public, unlisted, password-protected
- Cloudinary media library with drag-and-drop upload
- Companion web server for mobile access
- Umami analytics integration per post
- Configurable publish targets, editors, and vault paths via Settings UI

## Stack

- **Backend:** Tauri 2 (Rust)
- **Frontend:** Vue 3 + TypeScript
- **Preview:** Node.js server using website's remark/rehype pipeline

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/dmg/Dispatch_x.x.x_aarch64.dmg`

## Configuration

Settings are stored at `~/Library/Application Support/com.ejfox.dispatch/config.json` and editable via the in-app Settings UI (gear icon or `,` key).

### Vault
- **Vault path** — path to your Obsidian vault
- **Vault name** — used for `obsidian://` URL scheme
- **Excluded dirs** — folders to skip when scanning
- **Publishable dirs** — folders Dispatch will scan (default: `blog`, `drafts`)

### Publish Targets
Multiple website repos, each with its own domain, content path pattern, and git branch. One target is marked as default. When multiple targets are configured, a dropdown appears in the publish toolbar.

### Editors
Configure which editors appear in the toolbar (Obsidian, iA Writer, VS Code, or add your own). Set a default editor for the `i` keyboard shortcut.

### Connections
Cloudinary cloud name and analytics URL are configured here. API secrets (Cloudinary keys, Umami credentials) stay in `.env` — see `.env.example`.

## Dispatch pieces

Standalone journalism lives in the vault folder `dispatch/` (badged **DISPATCH** in the list). Publishing copies
`<vault>/dispatch/<slug>.md` to website2 `content/dispatch/<slug>.md` (no year folder) and returns
`https://ejfox.com/dispatch/<slug>`. Frontmatter: `title, dek, date, image, image_alt` (required with `image`),
`tags, sources, data, claims, syndication, unlisted, draft`.

## Omnipublish (Bluesky + Mastodon)

The Syndicate wizard's **Post now** fans a published note out to every selected network independently:

- **Bluesky** (AT Protocol): `createSession` → `uploadBlob` (card thumbnail) → `createRecord` with link/hashtag
  facets (UTF-8 byte offsets) and an `app.bsky.embed.external` link card. Text is kept ≤300 graphemes.
- **Mastodon**: image uploaded via `/api/v2/media` with alt text as `description`, then attached to the status.
- Up to 3 attempts per network with backoff on transient errors; one network failing never blocks another.
- Each resulting URL is written back into the vault note's `syndication: [{network, url}]` (minimal text edit), and a
  network already listed there is skipped — re-running never double-posts.
- A note with `image` but no `image_alt` gets alt text generated (alttext.rs) and saved first; images are never
  posted without alt text. Password-protected and `draft: true` notes are never syndicated; unlisted notes go to
  Mastodon as unlisted and skip Bluesky.
- **Dry run** (button, or `DISPATCH_SYNDICATE_DRY_RUN=1`) builds and shows every payload without any network call.

Credentials (`.env`): `BLUESKY_HANDLE`, `BLUESKY_APP_PASSWORD` (optional `BLUESKY_SERVICE`), `MASTODON_INSTANCE`,
`MASTODON_ACCESS_TOKEN`.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate files |
| `gg` / `G` | Top / bottom |
| `/` or `Cmd+K` | Search |
| `o` | Open in Obsidian |
| `i` | Open in default editor |
| `p` | Preview |
| `v` | View on site |
| `c` | Copy URL |
| `m` | Media library |
| `,` | Settings |
| `Cmd+Enter` | Publish |
| `?` | All shortcuts |

## Architecture

```
src-tauri/src/
├── config.rs      # App configuration (persisted JSON)
├── lib.rs         # Tauri commands + app setup
├── vault.rs       # Obsidian vault scanning
├── publish.rs     # Git publish/unpublish/schedule
├── cloudinary.rs  # Cloudinary CDN integration
├── asset_usage.rs # Track media usage across posts
├── companion.rs   # Companion web server (mobile)
├── analytics.rs   # Umami analytics
├── preview.rs     # Local preview server
└── obsidian.rs    # Obsidian REST API (backlinks)

src/components/
├── FileList.vue
├── FilePreview.vue
├── SettingsModal.vue
├── LocalMediaFixer.vue
└── Media/
    ├── MediaLibraryModal.vue
    ├── MediaLibrarySidebar.vue
    ├── MediaLibraryGrid.vue
    └── MediaLibraryDetail.vue
```
