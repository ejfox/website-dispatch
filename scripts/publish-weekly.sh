#!/bin/bash
# publish-weekly.sh
# Copies a weekly note from private week-notes/ to blog/week-notes/
# Transforms wiki-links, adds proper frontmatter
#
# Usage: publish-weekly 2026-04

set -e

VAULT_PATH="$HOME/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox"
WEEK="$1"

if [ -z "$WEEK" ]; then
  echo "Usage: publish-weekly <week>"
  echo "Example: publish-weekly 2026-04"
  exit 1
fi

SOURCE="$VAULT_PATH/week-notes/$WEEK.md"
DEST_DIR="$VAULT_PATH/blog/week-notes"
DEST="$DEST_DIR/$WEEK.md"

# Check source exists
if [ ! -f "$SOURCE" ]; then
  echo "❌ Not found: $SOURCE"
  exit 1
fi

# Create destination directory if needed
mkdir -p "$DEST_DIR"

# Check if already published
if [ -f "$DEST" ]; then
  echo "⚠️  Already exists: $DEST"
  read -p "Overwrite? [y/N] " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
  fi
fi

# Parse the week number for the title
# 2026-04 -> Week 04, 2026
YEAR="${WEEK:0:4}"
WEEK_NUM="${WEEK:5:2}"
TITLE="Week $WEEK_NUM, $YEAR"

# Process the file
{
  # New frontmatter
  echo "---"
  echo "title: \"$TITLE\""
  echo "date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo "type: week-note"
  echo "tags:"
  echo "  - week-notes"
  echo "---"
  echo ""

  # Skip original frontmatter, process content
  in_frontmatter=false
  frontmatter_done=false

  while IFS= read -r line || [ -n "$line" ]; do
    # Track frontmatter boundaries
    if [ "$frontmatter_done" = false ]; then
      if [ "$line" = "---" ]; then
        if [ "$in_frontmatter" = false ]; then
          in_frontmatter=true
          continue
        else
          frontmatter_done=true
          continue
        fi
      fi
      if [ "$in_frontmatter" = true ]; then
        continue
      fi
    fi

    # Convert wiki-links: [[path|text]] -> [text](/path)
    # Also handles [[path]] -> [path](/path)
    converted=$(echo "$line" | sed -E 's/\[\[([^]|]+)\|([^]]+)\]\]/[\2](\/\1)/g' | sed -E 's/\[\[([^]]+)\]\]/[\1](\/\1)/g')

    echo "$converted"
  done < "$SOURCE"

} > "$DEST"

echo "✅ Published: $DEST"
echo ""
echo "   Title: $TITLE"
echo "   Visibility: public (week-notes feed only, not main blog)"
echo ""
echo "Next: Open Dispatch to preview and publish"
