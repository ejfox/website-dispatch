// Unit tests for the pure inline-diff core (src/utils/inlineDiff.ts).
// Node 22.18+ strips TypeScript types natively, so this imports the .ts
// module directly:  node --test test/inline-diff.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { diffWords, diffBlocks, similarity, normalizeText } from '../src/utils/inlineDiff.ts'

const p = (text) => ({ tag: 'p', text })

/** Rebuild what the reader sees from segments: [-removed-] {+added+}. */
function render(newText, segments) {
  let out = ''
  let cursor = 0
  const sorted = [...segments].sort((a, b) => {
    const pa = a.type === 'removed' ? a.at : a.start
    const pb = b.type === 'removed' ? b.at : b.start
    return pa - pb || (a.type === 'removed' ? -1 : 1)
  })
  for (const s of sorted) {
    if (s.type === 'removed') {
      out += newText.slice(cursor, s.at) + `${s.lead}[-${s.text}-]${s.trail}`
      cursor = s.at
    } else {
      out += newText.slice(cursor, s.start)
      const t = newText.slice(s.start, s.end)
      out += s.type === 'added' ? `{+${t}+}` : t
      cursor = s.end
    }
  }
  return out + newText.slice(cursor)
}

test('word swap: live 125cc → vault 150cc', () => {
  const live = 'I rode a 125cc Honda Grom across town.'
  const vault = 'I rode a 150cc Honda Grom across town.'
  const { segments, added, removed } = diffWords(live, vault)
  assert.equal(added, 1)
  assert.equal(removed, 1)
  assert.equal(render(vault, segments), 'I rode a [-125cc-] {+150cc+} Honda Grom across town.')
})

test('removed words at the end of a block keep a leading space', () => {
  const { segments, removed } = diffWords('Fixed on the live site already.', 'Fixed on the live site')
  assert.equal(removed, 1)
  assert.equal(render('Fixed on the live site', segments), 'Fixed on the live site [-already.-]')
})

test('added range offsets index the NEW text, whitespace preserved', () => {
  const vault = 'one\n  two three'
  const { segments } = diffWords('one three', vault)
  const add = segments.find((s) => s.type === 'added')
  assert.equal(vault.slice(add.start, add.end), 'two')
})

test('identical text → only equal segments', () => {
  const { segments, added, removed } = diffWords('same words here', 'same words here')
  assert.equal(added + removed, 0)
  assert.ok(segments.every((s) => s.type === 'equal'))
})

test('blocks: unchanged, edited, inserted and deleted paragraphs', () => {
  const live = [p('Intro stays.'), p('Live has a typo fix here.'), p('Only on live.'), p('Outro.')]
  const vault = [p('Intro stays.'), p('Live has a typo here.'), p('Outro.'), p('Brand new ending.')]
  const { ops, wordsAdded, wordsRemoved } = diffBlocks(live, vault)
  assert.deepEqual(
    ops.map((o) => o.kind),
    ['equal', 'changed', 'removed', 'equal', 'added'],
  )
  assert.equal(ops[2].oldIndex, 2)
  // "fix" + "Only on live." removed; "Brand new ending." added.
  assert.equal(wordsRemoved, 1 + 3)
  assert.equal(wordsAdded, 3)
})

test('whitespace-only differences are not changes', () => {
  const { ops } = diffBlocks([p('a  b\nc')], [p('a b c')])
  assert.deepEqual(ops.map((o) => o.kind), ['equal'])
})

test('different tags never pair (heading vs paragraph)', () => {
  const { ops } = diffBlocks([{ tag: 'h2', text: 'Same words' }], [p('Same words')])
  assert.deepEqual(ops.map((o) => o.kind).sort(), ['added', 'removed'])
})

test('rewritten paragraph (low similarity) is removed + added, not word soup', () => {
  const { ops } = diffBlocks(
    [p('The quick brown fox jumps over the lazy dog')],
    [p('Completely different sentence with nothing shared at all')],
  )
  assert.deepEqual(ops.map((o) => o.kind), ['removed', 'added'])
})

test('atomic blocks (mermaid) are swapped whole', () => {
  const { ops } = diffBlocks(
    [{ tag: 'pre', text: 'graph TD; A-->B', atomic: true }],
    [{ tag: 'pre', text: 'graph TD; A-->C', atomic: true }],
  )
  assert.deepEqual(ops.map((o) => o.kind), ['removed', 'added'])
})

test('containers recurse: one list item edited, one dropped', () => {
  const li = (text) => ({ tag: 'li', text })
  const ul = (items) => ({ tag: 'ul', text: items.map((i) => i.text).join('\n'), children: items })
  const live = [ul([li('apples'), li('pears are good'), li('plums')])]
  const vault = [ul([li('apples'), li('pears are great')])]
  const { ops, wordsAdded, wordsRemoved } = diffBlocks(live, vault)
  assert.equal(ops.length, 1)
  assert.equal(ops[0].kind, 'changed')
  assert.deepEqual(ops[0].children.map((o) => o.kind), ['equal', 'changed', 'removed'])
  assert.equal(wordsAdded, 1) // great
  assert.equal(wordsRemoved, 2) // good, plums
})

test('similarity + normalizeText helpers', () => {
  assert.equal(normalizeText('  a \n b  '), 'a b')
  assert.equal(similarity('a b c d', 'a b c d'), 1)
  assert.equal(similarity('a b c d', 'a b x y'), 0.5)
  assert.equal(similarity('', ''), 1)
  // punctuation doesn't make words different for pairing purposes
  assert.equal(similarity('air filter, cleaned twice', 'air filter'), 0.5)
})
