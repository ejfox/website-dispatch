# Changelog

## 0.5.0 — 2026-05-05

Big batch — auto-updates, command palette, drag-in, plus structural cleanup.

### Distribution
- **Auto-updater wired.** `tauri-plugin-updater` checks GitHub Releases on
  startup; surfaces a sticky toast with "Install & Restart" when a new
  build is available. Updater signing keypair generated at
  `~/.tauri/dispatch.key` (back this up — losing it locks out existing
  installs from future updates).
- **GitHub Actions release workflow** (`.github/workflows/release.yml`).
  Tag `v*` → builds universal DMG, signs + notarizes via Apple Dev cert,
  signs the update artifact with the updater key, uploads to Release,
  auto-generates `latest.json`. See `docs/RELEASE_SETUP.md` for the
  one-time secrets setup. After that, `git tag` is the entire release
  ritual.

### UX
- **`⌘K` command palette.** Linear-style fuzzy search over actions
  (publish, refresh, panel switching, copy URL, open in Obsidian, etc.)
  AND recent files in one unified list. Subsequence matching, sectioned.
  ⌘K toggles, Esc closes, ↑↓ navigates, Enter executes.
- **Drag-and-drop file open.** Drop a `.md` from Finder onto Dispatch;
  if it's already in the file list, selects it; otherwise toasts the
  path. Non-`.md` drops surface a warning toast.
- **Toast/banner feedback system.** New `useToasts` composable + native
  HUD-style stack at the bottom. Replaces `alert()` in publish error
  paths and surfaces previously-buried OG generator errors with a
  "Copy Error" action.
- **System sounds** wired to publish events: `Hero` on milestone publish,
  `Glass` on regular publish, `Sosumi` on errors via the toast layer.
- **Fuller right-click menu on FileList.** Adds Reveal in Finder, Copy
  Public URL, Unpublish… (live posts only). Right-clicking now also
  selects the file so context matches what you clicked.

### Portability
- **`preview-server.mjs` is now bundled** into `.app` Resources, and its
  hardcoded `WEBSITE2_PATH = '/Users/ejfox/code/website2'` is now an env
  var that Dispatch passes in from the default publish target's
  `repo_path`. A portable Dispatch.app no longer secretly depends on
  the developer's filesystem.
- **`get_preview_server_path()`** prefers bundled paths over the dev
  tree (was the other way around in 0.4.3).

### Code health
- **Logging via `tauri-plugin-log`.** All 40+ `eprintln!`/`println!` calls
  replaced with `log::warn!`/`log::info!`. Log file at
  `~/Library/Logs/Dispatch/dispatch.log` (Console.app readable). Log
  macros can't panic on EPIPE, so the bundled app no longer crashes when
  its parent terminal dies (the 0.4.4 panic-hook stays as defense in depth).
- **Corrupt-SQLite recovery in `journal.rs`.** `init_db` now runs
  `PRAGMA integrity_check` on open; on failure, quarantines the bad file
  to `journal.corrupt-{ts}.db` (plus WAL/SHM siblings) and recreates
  fresh. Was: `expect("Failed to create journal schema")` → app abort
  with no recovery.
- **`bin_paths` module** resolves `node` and `git` via the user's login
  shell at startup, caches absolute paths. macOS GUI app PATH is the
  minimal launchctl one — bare `Command::new("node")` would fail on
  homebrew/nvm/asdf setups. Replaced ~25 call sites.

## 0.4.4 — 2026-05-05

### Fixed
- **Crash on arrow-key file navigation** when launched from a terminal that later closed. Root cause: `eprintln!` in `preview::set_file` would panic on EPIPE when stderr's parent process had exited, double-panic → SIGABRT. Two-part fix:
  - `preview::set_file` now swallows preview-server errors silently (the frontend handles missing-preview UI on its own).
  - `run()` installs a panic hook that suppresses `failed printing to stdout/stderr` panics globally, so the other 40+ `eprintln!` call sites can't take down the bundled app.

## 0.4.3 — 2026-05-05

### Fixed
- **Tray icon click did nothing.** Removed the duplicate declarative `trayIcon` block from `tauri.conf.json` — it was creating a second, inert tray icon alongside the working programmatic one. The Rust-built tray with menu + click handler is now the only one.
- **OG image generator failing for older posts.** `slug` derived from `props.file.filename` lost the year directory; it now extracts `<year>/<slug>` from the file path so `processed/<year>/<slug>.json` is found. `targetUrl` no longer hard-codes the current year either.

### Added
- **Native-feel CSS pass.**
  - `color-scheme: light dark` so native form controls render correctly in both modes.
  - SF Pro stylistic alts (`ss01`, `cv11`) + `font-optical-sizing: auto`.
  - Focus rings + `accent-color` now use system `AccentColor` (whatever the user picked in System Settings).
  - `prefers-reduced-motion` honored globally.
- **App menu fleshed out:** new `Publish` submenu (Publish, Publish Unlisted, Copy URL, Generate OG, Show Journal); `View` adds Toggle Sidebar + panel switchers ⌘1–4 + Fullscreen; new `Help` submenu (Help, Report Issue, View on GitHub).
- **`play_system_sound` Tauri command** — invokes `afplay` against `/System/Library/Sounds/*.aiff`. Call from JS for native feedback (`Glass`, `Sosumi`, `Pop`, `Tink`).

### Pinned
- `tauri = "~2.10.1"` to skip the 2.9.2 transparent-border regression ([tauri#14394](https://github.com/tauri-apps/tauri/issues/14394)).

### Internal
- **Lint pass:** zero warnings across `vue-tsc`, `prettier`, `cargo fmt`, `cargo clippy`. Refactored `journal::record_event` to take an `EventRecord` struct instead of 9 positional args.
- Fixed type union for `rightTab` in `useKeyboardShortcuts` to include `'gear'`.

## 0.4.2 — 2026-04-30

### Added
- **Gear panel** (right-panel tab next to Preview / Media / Journal). Daily-hygiene tracking for `~/code/website2/data/gear.csv`:
  - `u` — stamp `Last_Used` = today on selected item
  - `l` — edit `Location_Room` / `Location_Detail` inline
  - `s` — paste `Scan_3D_URL` (Polycam progress at a glance)
  - `e` — shell out to `gear-tui` for heavier edits
  - `c` — single batched commit (`gear: hygiene pass YYYY-MM-DD`)
- Pending-changes badge driven by real `git diff` against `data/gear.csv` (no in-memory bookkeeping).
- Stale `Last_Used` highlighted (>90 days or empty).
- New Tauri commands: `list_gear`, `mark_gear_used`, `update_gear_location`, `set_gear_scan_url`, `gear_pending_changes`, `commit_gear_changes`.

### Internal
- Added `csv` crate dep.
- New module `src-tauri/src/gear.rs`; CSV path auto-derives from `publish_targets[default].repo_path`.

## 0.4.1

- Cargo.lock refresh for release.
- CSS Grid layout system, dynamic version, UI cleanup.
- Restore dark mode background, fix titlebar button overlap at narrow widths.
- Remove window-state plugin config that caused crash on launch.

## 0.4.0

- Major codebase audit: modularize, optimize, harden, native macOS feel.
- OG image picker integrated; OG upload writes to website's `og-images.json`.
- Syndication wizard + promo image generation.
- Syndication queue (SQLite) with background scheduler.
- Alt text system + Mastodon syndication foundation.
- Crown button — one-click Vue page takeover scaffolding.
- Journal tab, streak titlebar, responsive media grid.
