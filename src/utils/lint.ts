// Which lint warnings stop a first publish. Mirrors `is_blocking_warning` in
// src-tauri/src/vault.rs. Only things that would actually break the live post
// block; the rest are worth knowing about and shouldn't stand in your way.

const ADVISORY = ['Modified since publish', 'Has TODOs', 'Long link text', 'Images to upload']

export function isBlockingWarning(w: string): boolean {
  return !ADVISORY.includes(w) && !w.startsWith('Missing alt text')
}
