# Changelog

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
