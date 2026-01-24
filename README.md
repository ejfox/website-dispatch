# Dispatch

A menubar app for curating and publishing Obsidian notes to ejfox.com.

## Features

### File Browser
- Scans `blog/` and `drafts/` folders in your Obsidian vault
- Sorted by most recently modified
- Shows published status (LIVE badge) for posts already on the website
- Turbo-style age visualization (yellow = fresh, purple = old)
- Displays word count, warnings, and publish dates

### ⌘K Quick Search
- Press `⌘K` to open search
- Searches across titles, filenames, and tags
- Arrow keys to navigate, Enter to select, Esc to close

### Live Preview
- Uses the **exact same markdown pipeline** as the production website
- All plugins: wikilinks, syntax highlighting, blockquotes, footnotes, GFM tables
- Auto-refreshes on file changes (polls every 400ms)
- Click "Preview" button to open at `http://localhost:6419`

### Quick Open
- **Obsidian** - Opens the file in your vault
- **iA Writer** - Opens in iA Writer
- **nvim** - Opens in iTerm/Terminal with neovim

### Publishing
- One-click publish to website repo
- Copies file to `website2/content/blog/YEAR/`
- Auto-commits with descriptive message
- Shows live URL after publishing

## Architecture

```
website-dispatch/
├── src/                    # Vue frontend
│   ├── App.vue            # Main app + ⌘K search
│   └── components/
│       ├── FileList.vue   # Sidebar file browser
│       └── FilePreview.vue # File details + actions
├── src-tauri/             # Rust backend
│   └── src/
│       ├── main.rs        # Tauri commands
│       ├── vault.rs       # Obsidian vault scanning
│       ├── publish.rs     # Git publish workflow
│       ├── preview.rs     # Node.js preview server manager
│       └── obsidian.rs    # Obsidian REST API integration
└── preview-server.mjs     # Node.js preview server
```

## Preview Pipeline

The preview server uses the **exact same unified/remark/rehype pipeline** as the production blog:

```
website2/scripts/markdownToHtml.mjs
  ├── remarkParse
  ├── remarkGfm (footnotes, tables, task lists)
  ├── remarkExtractToc
  ├── remarkObsidianSupport (wikilinks → internal links)
  ├── remarkEnhanceImages
  ├── remarkEnhanceLinks
  ├── remarkAi2htmlEmbed
  ├── remarkRehype
  ├── rehypeRaw
  ├── rehypePrettyCode (syntax highlighting with ayu-mirage theme)
  ├── rehypeAddClassToParagraphs
  ├── rehypeSlug
  └── rehypeStringify
```

## Development

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Requirements

- Node.js 18+
- Rust/Cargo
- Tauri CLI
- Obsidian vault at `~/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox`
- Website repo at `~/code/website2`

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `⌘K` | Open search |
| `↑↓` | Navigate search results |
| `Enter` | Select result |
| `Esc` | Close search |

## Config

Paths are configured in `src-tauri/src/main.rs`:

```rust
Config {
    vault_path: "~/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox",
    website_repo: "~/code/website2",
    excluded_dirs: vec!["week-notes", "robot-notes", "private", "templates", "attachments"],
}
```

## Safety Rails

### Excluded by default
- `week-notes/` - Personal weekly logs
- `robot-notes/` - AI-generated content
- `private/` - Explicitly private
- `templates/` - Obsidian templates
- `_stale/` and `_archive/` folders

### Warnings shown
- No title (# or ## heading)
- No date in frontmatter
- Contains `TODO` or `FIXME`
- Local images not uploaded to Cloudinary
