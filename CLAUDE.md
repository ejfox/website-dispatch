# Dispatch - Publishing Intentions

## The Vision

**One back door.** The Obsidian vault is a private thinking space. The only way content becomes public is by consciously placing it in `blog/` and pressing publish.

## Taxonomy: Location + Visibility

**Two concepts, cleanly separated:**

1. **Location** = Can it be published?
2. **Frontmatter** = How visible is it after publishing?

### Location (Folder) → Publishability

| Folder | Dispatch sees it? | Meaning |
|--------|-------------------|---------|
| `blog/` | **Yes** | Intended for publication |
| `drafts/` | **No** | Private thinking, never publish |
| `week-notes/` | **No** | Personal reflections |
| Everything else | **No** | Private vault content |

### Frontmatter → Visibility (after publishing)

```yaml
# Shareable but hidden from listings
unlisted: true

# Password-protected (implies unlisted)
password: birdsarentreal

# Fully public (default - no field needed)
# (nothing)
```

| Frontmatter | URL works? | Requires password? | In listings? |
|-------------|------------|-------------------|--------------|
| (nothing) | Yes | No | Yes |
| `unlisted: true` | Yes | No | No |
| `password: xyz` | Yes | Yes | No |

**Password protection:**
- Simple client-side gate (not high-security, but keeps casual visitors out)
- Share link + password with friend: "here's the draft, password is `birdsarentreal`"
- `password:` implies `unlisted:` — no need to specify both
- Password stored as bcrypt hash in processed JSON (not plaintext)

### Deprecated Fields

- **`draft: true`** — Remove. Location now handles this.
- Any other publish/status fields that overlap with folder semantics.

### The Mental Model

| I want to... | Put it in... | Frontmatter |
|--------------|--------------|-------------|
| Think privately | `drafts/` | (doesn't matter) |
| Share with specific person | `blog/` | `password: secretword` |
| Share draft (anyone with link) | `blog/` | `unlisted: true` |
| Publish publicly | `blog/` | (nothing) |

**Privacy spectrum:**
```
Most private                                      Most public
     │                                                 │
     ▼                                                 ▼
 drafts/    →    password:    →    unlisted:    →    (public)
 (never           (link +          (link only)       (everywhere)
  published)      password)
```

## The Ideal Workflow

### Daily Idea Essays (2026 Goal)
```
1. Have an idea
2. Create file directly in blog/2026/idea-name.md
3. Write
4. Open Dispatch
5. One click → live in seconds
6. Shareable link in hand
```

No intermediate steps. No "move from drafts." Write where you publish.

### Feedback Before Publishing (Rare)
```
1. Write in blog/ as usual
2. Use Dispatch Preview to show reviewer
3. Get feedback
4. Publish when ready
```

## Current State vs Desired State

### Implemented ✅
- [x] `drafts/` excluded from Dispatch scanning
- [x] Only `blog/` folder triggers Dispatch visibility
- [x] Clear mental model: `blog/` = public intent, everything else = private
- [x] Daily publishing is frictionless: write → click → live
- [x] `unlisted: true` frontmatter support with UI badges
- [x] `password: xyz` frontmatter support with Copy + Pass button
- [x] Color-coded visibility states (green=public, indigo=unlisted, purple=protected)
- [x] website2 filters protected content from feeds, sitemap, search, suggestions
- [x] Safety linter catches typos (`unlsted`, `pasword`) before publishing

### Remaining
- [x] Quick-create "New Post" button from Dispatch
- [ ] Clean up legacy `draft: true` frontmatter in vault

## Required Changes

### 1. Exclude drafts from scanning (Dispatch)
**File:** `src-tauri/src/main.rs` (Config default)

Add `"drafts"` to `excluded_dirs`:
```rust
excluded_dirs: vec![
    "week-notes".into(),
    "robot-notes".into(),
    "private".into(),
    "templates".into(),
    "attachments".into(),
    "drafts".into(),  // ADD THIS
],
```

### 2. Support `unlisted` and `password` frontmatter in Dispatch UI

**File:** `src-tauri/src/vault.rs` (or wherever frontmatter is parsed)

- Parse `unlisted: true` from frontmatter
- Parse `password: xyz` from frontmatter
- Show visual indicators in file list:
  - "UNLISTED" badge (gray/subtle)
  - "PROTECTED" badge (lock icon, maybe purple/blue)
- Publish button works the same for all

**File:** `src/components/FilePreview.vue`

- Show visibility status in info panel:
  - Public: (default, no special indicator)
  - Unlisted: "Unlisted - only people with link can find it"
  - Password: "Password-protected - requires password to view"
- Different banner colors:
  - Live + Public: Green
  - Live + Unlisted: Gray/muted
  - Live + Password: Purple/blue with lock icon
- After publish, show shareable info:
  - Unlisted: "Share this link (not indexed)"
  - Password: "Share this link + password: `{password}`"

**Copy actions:**
- "Copy URL" — copies just the URL
- "Copy URL + Password" (for protected posts) — copies formatted message:
  ```
  Here's the draft: https://ejfox.com/blog/2026/my-post
  Password: birdsarentreal
  ```

### 3. Support `unlisted` and `password` in website2

**Unlisted posts:**
- `pages/blog/index.vue` — Filter out `unlisted: true` from listing
- `server/routes/rss.xml.ts` (or equivalent) — Exclude unlisted from RSS
- `scripts/processMarkdown.mjs` — Preserve `unlisted` in processed JSON
- Blog post template — Add `<meta name="robots" content="noindex">` for unlisted posts
- Sitemap generation — Exclude unlisted posts

**Password-protected posts:**
- `scripts/processMarkdown.mjs` — Hash password with bcrypt, store hash (never plaintext)
- `pages/blog/[...slug].vue` — Check for `passwordHash` in post metadata
- New component: `PasswordGate.vue`
  - Renders overlay/modal asking for password
  - Client-side bcrypt check against stored hash
  - On success: reveal content, maybe store in sessionStorage for that post
  - On failure: "Wrong password, try again"
- Password-protected posts are implicitly unlisted (exclude from index/RSS/sitemap)
- Add noindex meta tag

**PasswordGate UX:**
```
┌─────────────────────────────────────┐
│                                     │
│   This post is password-protected   │
│                                     │
│   ┌─────────────────────────────┐   │
│   │ Enter password...           │   │
│   └─────────────────────────────┘   │
│                                     │
│          [ View Post ]              │
│                                     │
└─────────────────────────────────────┘
```

### 4. Deprecate `draft` frontmatter (Gradual)
**Status:** `draft: true` is still supported for backwards compatibility, but deprecated.

**Migration strategy:**
- New posts should NOT use `draft: true`
- Location determines publishability (blog/ vs drafts/)
- Existing code still checks for `draft` but this is redundant
- Vault AI should gradually migrate files by removing `draft: true` from frontmatter
- Full removal of `draft` checks can happen after migration is complete

**Files that still check `draft`:** (20+ files in website2)
- usePostFilters.ts - filters out draft posts from listings
- rss.xml.ts, feed.json.ts - exclude drafts from feeds
- nuxt.config.ts - prerendering excludes drafts
- Many other listing/filtering files

**No immediate code changes needed** - the system works with both old and new models.

### 5. (Future) Quick-create from Dispatch
Add a "New Post" button that:
- Prompts for title/slug
- Asks: Public or Unlisted?
- Creates `blog/{year}/{slug}.md` with appropriate frontmatter
- Opens in Obsidian or iA Writer
- Reduces friction for daily idea essays

## Design Principles

1. **The vault is sacred** - Private thinking space, not a publishing queue
2. **One back door** - Only `blog/` leads to public
3. **Explicit intent** - You put it in `blog/` because you mean to publish it
4. **Minimal friction** - Daily publishing should feel effortless
5. **No anxiety** - Writing anywhere except `blog/` is completely safe

## Git/Privacy Notes

- The Obsidian vault itself is NOT in a public repo
- Only content that gets published (copied to website2) becomes public
- The website2 repo contains processed content, not the vault
- Draft sharing uses local preview server, not public URLs

---

*This document captures editorial intent. Code changes should close the delta between current behavior and these intentions.*
