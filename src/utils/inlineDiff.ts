/**
 * Inline diff — the pure, DOM-free core of the Preview pane's Diff mode.
 *
 * Both versions of a post (the published copy in the website repo = OLD, the
 * vault note = NEW) are rendered through the same markdown renderer, then cut
 * into blocks (paragraphs, headings, list items…). This module:
 *
 *   1. aligns the two block lists (LCS on tag + normalized text),
 *   2. pairs up changed blocks of the same kind and word-diffs them,
 *   3. recurses into containers (lists, blockquotes, footnotes) so a change in
 *      one list item doesn't mark the whole list.
 *
 * Direction is always "vault → live", i.e. what publishing would do:
 * `added` = in the vault, not live yet; `removed` = live now, publishing would
 * delete it. Same direction as publish_diff.rs (`from_lines(published, source)`)
 * so the two diff views never disagree.
 *
 * Word segments are expressed as character offsets into the NEW block's text
 * (the concatenation of its text nodes), so the DOM adapter can mark them up
 * without touching links, footnote refs or other markup.
 */
import { diffArrays } from 'diff'

export interface DiffBlock {
  /** Lowercase tag name, e.g. 'p', 'h2', 'li'. */
  tag: string
  /** Concatenated text of the block (exactly the text nodes, in order). */
  text: string
  /** Never word-diff (mermaid source, svg…): changed = removed + added. */
  atomic?: boolean
  /** Present for containers (ul/ol/blockquote/…) whose children are blocks. */
  children?: DiffBlock[]
}

/** A run of text in the new block: unchanged, or added by the vault. */
export interface RangeSegment {
  type: 'equal' | 'added'
  start: number
  end: number
}
/** Live-only words, shown as a ghost inserted at offset `at` of the new text. */
export interface RemovedSegment {
  type: 'removed'
  at: number
  text: string
  /** Plain (unstruck) whitespace to place before / after the ghost. */
  lead: string
  trail: string
}
export type Segment = RangeSegment | RemovedSegment

export type BlockOp =
  | { kind: 'equal'; oldIndex: number; newIndex: number }
  | {
      kind: 'changed'
      oldIndex: number
      newIndex: number
      /** Word-level changes (leaf blocks). */
      segments?: Segment[]
      /** Block-level changes (container blocks). */
      children?: BlockOp[]
    }
  | { kind: 'added'; newIndex: number }
  | { kind: 'removed'; oldIndex: number }

export interface InlineDiffResult {
  ops: BlockOp[]
  wordsAdded: number
  wordsRemoved: number
}

/** Below this share of common words, a "changed" pair reads better as a
 *  whole removed block followed by a whole added block. */
const MIN_SIMILARITY = 0.3

export function normalizeText(s: string): string {
  return s.replace(/\s+/g, ' ').trim()
}

export function countWords(s: string): number {
  const m = s.match(/\S+/g)
  return m ? m.length : 0
}

function signature(b: DiffBlock): string {
  return `${b.tag}\u0000${normalizeText(b.text)}`
}

interface Word {
  w: string
  start: number
  end: number
}

function words(s: string): Word[] {
  const out: Word[] = []
  const re = /\S+/g
  let m: RegExpExecArray | null
  while ((m = re.exec(s))) out.push({ w: m[0], start: m.index, end: m.index + m[0].length })
  return out
}

/** Share of words the two texts have in common (0..1), by LCS. */
export function similarity(a: string, b: string): number {
  // Compare bare words: "filter," and "filter" are the same word here.
  const bare = (s: string) =>
    normalizeText(s)
      .toLowerCase()
      .split(' ')
      .map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ''))
      .filter(Boolean)
  const wa = bare(a)
  const wb = bare(b)
  const max = Math.max(wa.length, wb.length)
  if (max === 0) return 1
  let common = 0
  for (const part of diffArrays(wa, wb)) if (!part.added && !part.removed) common += part.count ?? part.value.length
  return common / max
}

/**
 * Word-level diff of two texts. Returns segments over `newText` plus counts.
 */
export function diffWords(
  oldText: string,
  newText: string,
): { segments: Segment[]; added: number; removed: number } {
  const ow = words(oldText)
  const nw = words(newText)
  const parts = diffArrays(
    ow.map((w) => w.w),
    nw.map((w) => w.w),
  )
  const segments: Segment[] = []
  let oi = 0
  let ni = 0
  let added = 0
  let removed = 0
  for (const part of parts) {
    const n = part.count ?? part.value.length
    if (part.removed) {
      const first = ow[oi]
      const last = ow[oi + n - 1]
      const text = normalizeText(oldText.slice(first.start, last.end))
      // Ghost sits right before the next new word (or at the end of the block).
      const atEnd = ni >= nw.length
      const at = atEnd ? newText.length : nw[ni].start
      segments.push({
        type: 'removed',
        at,
        text,
        lead: atEnd && newText.length > 0 && /\S$/.test(newText) ? ' ' : '',
        trail: atEnd ? '' : ' ',
      })
      removed += n
      oi += n
    } else {
      const first = nw[ni]
      const last = nw[ni + n - 1]
      segments.push({ type: part.added ? 'added' : 'equal', start: first.start, end: last.end })
      if (part.added) added += n
      else oi += n
      ni += n
    }
  }
  return { segments, added, removed }
}

function blockWords(b: DiffBlock): number {
  return countWords(b.text)
}

/**
 * Align two block lists and diff them. Pure: no DOM, no I/O.
 *
 * `ops` is in reading order of the merged document: removed (live-only)
 * blocks appear where they used to sit, right before the next vault block.
 */
export function diffBlocks(oldBlocks: DiffBlock[], newBlocks: DiffBlock[]): InlineDiffResult {
  const ops: BlockOp[] = []
  let wordsAdded = 0
  let wordsRemoved = 0

  const parts = diffArrays(oldBlocks.map(signature), newBlocks.map(signature))
  let oi = 0
  let ni = 0
  // Pending unmatched runs, flushed at the next equal block (or the end).
  let runOld: number[] = []
  let runNew: number[] = []

  const pushRemoved = (i: number) => {
    ops.push({ kind: 'removed', oldIndex: i })
    wordsRemoved += blockWords(oldBlocks[i])
  }
  const pushAdded = (i: number) => {
    ops.push({ kind: 'added', newIndex: i })
    wordsAdded += blockWords(newBlocks[i])
  }

  const canPair = (o: DiffBlock, n: DiffBlock) =>
    o.tag === n.tag &&
    !o.atomic &&
    !n.atomic &&
    // A container only pairs with a container (and a leaf with a leaf).
    !!o.children === !!n.children &&
    similarity(o.text, n.text) >= MIN_SIMILARITY

  const pushChanged = (o: number, n: number) => {
    const ob = oldBlocks[o]
    const nb = newBlocks[n]
    if (ob.children && nb.children) {
      const sub = diffBlocks(ob.children, nb.children)
      wordsAdded += sub.wordsAdded
      wordsRemoved += sub.wordsRemoved
      ops.push({ kind: 'changed', oldIndex: o, newIndex: n, children: sub.ops })
    } else {
      const wd = diffWords(ob.text, nb.text)
      wordsAdded += wd.added
      wordsRemoved += wd.removed
      ops.push({ kind: 'changed', oldIndex: o, newIndex: n, segments: wd.segments })
    }
  }

  const flush = () => {
    // Greedy, order-preserving pairing of same-kind blocks within the run.
    const pairs: [number, number][] = [] // [run-old idx, run-new idx]
    let p = 0
    runNew.forEach((n, ni) => {
      for (let j = p; j < runOld.length; j++) {
        if (canPair(oldBlocks[runOld[j]], newBlocks[n])) {
          pairs.push([j, ni])
          p = j + 1
          return
        }
      }
    })
    // Emit gap by gap: live-only blocks first (where they used to sit), then
    // vault-only blocks, then the edited pair — reads like old → new.
    let oc = 0
    let nc = 0
    for (const [oj, nj] of [...pairs, [runOld.length, runNew.length] as [number, number]]) {
      for (; oc < oj; oc++) pushRemoved(runOld[oc])
      for (; nc < nj; nc++) pushAdded(runNew[nc])
      if (oj < runOld.length) {
        pushChanged(runOld[oj], runNew[nj])
        oc++
        nc++
      }
    }
    runOld = []
    runNew = []
  }

  for (const part of parts) {
    const n = part.count ?? part.value.length
    if (part.removed) {
      for (let k = 0; k < n; k++) runOld.push(oi++)
    } else if (part.added) {
      for (let k = 0; k < n; k++) runNew.push(ni++)
    } else {
      flush()
      for (let k = 0; k < n; k++) ops.push({ kind: 'equal', oldIndex: oi++, newIndex: ni++ })
    }
  }
  flush()
  return { ops, wordsAdded, wordsRemoved }
}
