# Dispatch

Tauri desktop app for publishing Obsidian notes to ejfox.com.

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
