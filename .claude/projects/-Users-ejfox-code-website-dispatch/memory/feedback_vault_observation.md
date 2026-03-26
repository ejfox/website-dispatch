---
name: Vault observation boundary
description: Dispatch can observe/index the full vault for intelligence, but must NEVER present draft content near publish actions — vault insight UI must feel like a dashboard, not a staging area
type: feedback
---

Dispatch should gain situational awareness of the full Obsidian vault (drafts, notes, everything) to surface intelligent suggestions — but the UI must maintain a hard boundary between "observing your private work" and "publishing."

**Why:** The user's vault is their private thinking space. Seeing a draft filename near a publish button creates anxiety about accidental publishing. The existing "one back door" principle (only blog/ leads to public) must extend to the UI — vault insights should never feel like a publishing queue.

**How to apply:**
- Vault intelligence (draft activity, tag clusters, editing patterns) should live in its own UI context — the journal tab or a "signals" section — never in the file list or publish flow
- Language should always be observational: "you've been working on..." not "ready to publish?"
- No action buttons (publish, schedule, etc.) should appear near vault-only content
- The file list should ONLY show publishable-dir files as it does today
- If suggesting a draft be moved to blog/, frame it as the user's decision: "this draft has grown — want to move it to blog/ when you're ready?"
