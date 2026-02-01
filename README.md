# Dispatch

Tauri desktop app for publishing Obsidian notes to ejfox.com.

<img width="900" height="1131" alt="screenshot 2026-01-26 at 11 40 14 AM" src="https://github.com/user-attachments/assets/6964af9d-aa81-4184-85d3-e0a8ef27c27f" />

## Features

- Scans Obsidian vault (`blog/` and `drafts/` folders)
- Live preview using exact website markdown pipeline
- One-click publish with git commit/push
- Tracks LIVE / MODIFIED / draft status via content diff
- Keyboard-driven navigation (j/k, gg/G, 1-9, ? for help)
- Filter: All / Live / Drafts
- Sort: Modified / Created / Title / Words
- Native preview window with hot-reload

## Stack

- **Backend:** Tauri (Rust)
- **Frontend:** Vue 3 + TypeScript
- **Preview:** Node.js server using website2's remark/rehype pipeline

## Media Library Components

The Cloudinary media library UI was split into a dedicated folder to make it easier to extend into a full-featured media management tool. The modal stays as the stateful container (loading, keyboard handling, selection, Cloudinary calls), and the UI is broken into focused subcomponents (sidebar, grid, detail). This keeps behavior centralized while letting the UI evolve without a single monolithic file.

- Container: `src/components/Media/MediaLibraryModal.vue`
- Sidebar: `src/components/Media/MediaLibrarySidebar.vue`
- Grid: `src/components/Media/MediaLibraryGrid.vue`
- Detail panel: `src/components/Media/MediaLibraryDetail.vue`

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

## Config

Edit `src-tauri/src/main.rs` to change:
- `vault_path` - Obsidian vault location
- `website_repo` - website2 repo location
