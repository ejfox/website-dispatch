import test from 'node:test'
import assert from 'node:assert/strict'
import { isBlockingWarning } from '../src/utils/lint.ts'
import { slugifyTitle } from '../src/utils/slug.ts'

test('only things that break the live post block a first publish', () => {
  for (const w of ['Has TODOs', 'Missing alt text (2)', 'Modified since publish', 'Long link text', 'Images to upload']) {
    assert.equal(isBlockingWarning(w), false, w)
  }
  for (const w of ['Local images', 'Broken link', 'No date', '[privacy]Phone number detected']) {
    assert.equal(isBlockingWarning(w), true, w)
  }
})

test('the New Post path preview matches the slug Rust makes', () => {
  assert.equal(slugifyTitle("Don't Stop"), 'don-t-stop')
  assert.equal(slugifyTitle('  Coach   Artie: 15k people! '), 'coach-artie-15k-people')
  assert.equal(slugifyTitle('Café Déjà'), 'café-déjà')
  assert.equal(slugifyTitle('!!!'), '')
})
