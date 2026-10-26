/**
 * DOM adapter for the inline diff (see inlineDiff.ts for the pure core).
 *
 * Takes the rendered HTML of the published copy (OLD) and the vault note
 * (NEW), cuts both into DiffBlocks, runs `diffBlocks`, then annotates the NEW
 * document in place:
 *   - added words → <ins class="diff-ins">, added blocks → .diff-block-added
 *   - live-only words → <del class="diff-del"> ghosts, live-only blocks are
 *     cloned in from the old render as .diff-block-removed
 *   - every changed block gets .diff-changed (the margin tick hangs off the
 *     top-level ones)
 * Only text nodes are split, so links, footnote refs and inline markup in the
 * vault render survive untouched.
 */
import { diffBlocks, type BlockOp, type DiffBlock, type Segment } from './inlineDiff'

const BLOCK_TAGS = new Set([
  'p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'blockquote', 'pre',
  'table', 'section', 'div', 'figure', 'hr', 'dl', 'dt', 'dd', 'details', 'aside',
  'header', 'footer', 'img', 'video', 'iframe', 'svg',
])
/** Containers we descend into when all their content is blocks. */
const CONTAINER_TAGS = new Set(['ul', 'ol', 'li', 'blockquote', 'section', 'div', 'details', 'aside'])
const SKIP_TEXT_IN = new Set(['SCRIPT', 'STYLE'])

function textNodes(root: Node): Text[] {
  const out: Text[] = []
  const walker = root.ownerDocument!.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      for (let p = n.parentElement; p && p !== root; p = p.parentElement) {
        if (SKIP_TEXT_IN.has(p.tagName)) return NodeFilter.FILTER_REJECT
      }
      return NodeFilter.FILTER_ACCEPT
    },
  })
  while (walker.nextNode()) out.push(walker.currentNode as Text)
  return out
}

function blockText(el: Element): string {
  return textNodes(el)
    .map((t) => t.data)
    .join('')
}

function isAtomic(el: Element): boolean {
  const tag = el.tagName.toLowerCase()
  if (tag === 'svg' || tag === 'iframe' || tag === 'video' || tag === 'img') return true
  return el.classList.contains('mermaid') || !!el.querySelector('.mermaid, svg')
}

/** Element children if every non-blank child node is a block element. */
function blockChildren(el: Element): Element[] | null {
  const kids: Element[] = []
  for (const node of Array.from(el.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) {
      if ((node as Text).data.trim()) return null
      continue
    }
    if (node.nodeType !== Node.ELEMENT_NODE) continue
    const child = node as Element
    if (!BLOCK_TAGS.has(child.tagName.toLowerCase())) return null
    kids.push(child)
  }
  return kids.length ? kids : null
}

interface Extracted {
  blocks: DiffBlock[]
  els: Element[]
  /** Child extraction per container block, aligned with `blocks`. */
  sub: (Extracted | null)[]
}

function extract(parent: Element, root = false): Extracted {
  // The document root is taken leniently: every element child is a block
  // (stray inline HTML at top level still diffs, as its own block).
  const els = root ? Array.from(parent.children) : (blockChildren(parent) ?? [])
  const blocks: DiffBlock[] = []
  const sub: (Extracted | null)[] = []
  for (const el of els) {
    const tag = el.tagName.toLowerCase()
    const atomic = isAtomic(el)
    let inner: Extracted | null = null
    if (!atomic && CONTAINER_TAGS.has(tag) && blockChildren(el)) inner = extract(el)
    blocks.push({
      tag,
      text: blockText(el),
      atomic: atomic || undefined,
      children: inner ? inner.blocks : undefined,
    })
    sub.push(inner)
  }
  return { blocks, els, sub }
}

function applySegments(el: Element, segments: Segment[]) {
  const doc = el.ownerDocument
  const nodes = textNodes(el)
  const total = nodes.reduce((n, t) => n + t.data.length, 0)
  const ins = segments.filter((s) => s.type === 'added') as { start: number; end: number }[]
  const dels = segments.filter((s) => s.type === 'removed') as {
    at: number
    text: string
    lead: string
    trail: string
  }[]

  // Block with no text at all (e.g. text added to an empty paragraph can't
  // happen, but a removed-only paragraph could): append ghosts at the end.
  if (!nodes.length) {
    for (const d of dels) el.appendChild(makeDel(doc, d.text))
    return
  }

  let offset = 0
  nodes.forEach((node, idx) => {
    const s = offset
    const e = offset + node.data.length
    offset = e
    const isLast = idx === nodes.length - 1
    const myDels = dels.filter((d) => (d.at >= s && d.at < e) || (isLast && d.at === total))
    const myIns = ins
      .map((r) => ({ start: Math.max(r.start, s), end: Math.min(r.end, e) }))
      .filter((r) => r.end > r.start)
    if (!myDels.length && !myIns.length) return

    const points = new Set<number>([s, e])
    for (const d of myDels) points.add(d.at)
    for (const r of myIns) {
      points.add(r.start)
      points.add(r.end)
    }
    const sorted = [...points].sort((a, b) => a - b)
    const frag = doc.createDocumentFragment()
    for (let i = 0; i < sorted.length; i++) {
      const p = sorted[i]
      for (const d of myDels.filter((d) => d.at === p)) {
        if (d.lead) frag.appendChild(doc.createTextNode(d.lead))
        frag.appendChild(makeDel(doc, d.text))
        if (d.trail) frag.appendChild(doc.createTextNode(d.trail))
      }
      const next = sorted[i + 1]
      if (next === undefined || next <= p) continue
      const slice = node.data.slice(p - s, next - s)
      if (myIns.some((r) => r.start <= p && next <= r.end)) {
        const m = doc.createElement('ins')
        m.className = 'diff-ins'
        m.textContent = slice
        frag.appendChild(m)
      } else {
        frag.appendChild(doc.createTextNode(slice))
      }
    }
    node.replaceWith(frag)
  })
}

function makeDel(doc: Document, text: string): HTMLElement {
  const d = doc.createElement('del')
  d.className = 'diff-del'
  d.textContent = text
  return d
}

function stripIds(el: Element) {
  el.removeAttribute('id')
  el.querySelectorAll('[id]').forEach((n) => n.removeAttribute('id'))
}

function applyOps(parent: Element, ops: BlockOp[], oldX: Extracted, newX: Extracted): boolean {
  const doc = parent.ownerDocument
  let pending: Element[] = []
  let changed = false
  const flushBefore = (anchor: Element | null) => {
    for (const r of pending) parent.insertBefore(r, anchor)
    pending = []
  }
  for (const op of ops) {
    if (op.kind === 'removed') {
      const ghost = doc.importNode(oldX.els[op.oldIndex], true) as Element
      stripIds(ghost)
      ghost.classList.add('diff-block-removed', 'diff-changed')
      ghost.setAttribute('aria-label', 'Live only: publishing would remove this')
      pending.push(ghost)
      changed = true
      continue
    }
    const el = newX.els[op.newIndex]
    flushBefore(el)
    if (op.kind === 'added') {
      el.classList.add('diff-block-added', 'diff-changed')
      changed = true
    } else if (op.kind === 'changed') {
      if (op.children) {
        applyOps(el, op.children, oldX.sub[op.oldIndex]!, newX.sub[op.newIndex]!)
      } else if (op.segments) {
        applySegments(el, op.segments)
      }
      el.classList.add('diff-changed')
      changed = true
    }
  }
  // Trailing removed blocks: append after the last vault block in this
  // parent (inside the container, never after unrelated trailing nodes).
  const lastEl = newX.els[newX.els.length - 1]
  flushBefore(lastEl ? lastEl.nextSibling as Element | null : null)
  return changed
}

export interface InlineDiffHtml {
  html: string
  wordsAdded: number
  wordsRemoved: number
  /** Top-level blocks carrying a change (for "N changed blocks" / nav). */
  changedBlocks: number
}

/** Diff two rendered HTML strings; returns the annotated vault HTML. */
export function buildInlineDiffHtml(liveHtml: string, vaultHtml: string): InlineDiffHtml {
  const parser = new DOMParser()
  const oldDoc = parser.parseFromString(`<body>${liveHtml}</body>`, 'text/html')
  const newDoc = parser.parseFromString(`<body>${vaultHtml}</body>`, 'text/html')
  const oldX = extract(oldDoc.body, true)
  const newX = extract(newDoc.body, true)
  const result = diffBlocks(oldX.blocks, newX.blocks)
  applyOps(newDoc.body, result.ops, oldX, newX)
  const changedBlocks = newDoc.body.querySelectorAll(':scope > .diff-changed').length
  return {
    html: newDoc.body.innerHTML,
    wordsAdded: result.wordsAdded,
    wordsRemoved: result.wordsRemoved,
    changedBlocks,
  }
}
