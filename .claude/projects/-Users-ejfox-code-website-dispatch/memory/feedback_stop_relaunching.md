---
name: Stop relaunching dev server
description: Don't keep killing and restarting tauri dev — Vite hot-reloads Vue changes, only relaunch when Rust code changes
type: feedback
---

Stop relaunching `tauri dev` after every change. Vite hot-reloads Vue/CSS changes automatically. Only relaunch when Rust backend code changes (new Tauri commands, schema changes, etc). The user finds the constant rebuilding annoying.

**Why:** The user said "stop rebuilding the app and just make it good dude" — they want focused code work, not process management.

**How to apply:** After making frontend-only changes, don't touch the dev server. After Rust changes, mention that a restart is needed but let the user handle it. Focus on writing good code.
