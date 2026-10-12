<script setup lang="ts">
import { watch, onUnmounted } from 'vue'
import { PhCursor, PhKeyboard, PhEye, PhCheckCircle, PhNewspaper, PhRocket, PhSquaresFour } from '@phosphor-icons/vue'
import { useAppConfig } from '../composables/useVaultData'
import { CHANGED_LABEL, SITE_PREVIEW_LABEL } from '../utils/vocab'

// One overlay, two pages: how publishing works, and every shortcut. `?`,
// ⌘/ and Help › Keyboard Shortcuts open the shortcuts; Help › How Dispatch
// Publishes opens the overview. Each page links to the other.
//
// The overview is written from the code, not from memory. If you change
// what publishing does, change it here too:
//   publish.rs        publish_file / unpublish_file / run_schedule_checker
//   publish_guard.rs  what is never publishable
//   usePublishFlow.ts deploy watcher (DEPLOYING… → LIVE ✓ / NOT LIVE),
//                     webmentions on live
//   website2          utils/postFilters.ts publishTime (publishAt / date),
//                     .github/workflows/{send-webmentions,mirror-atproto}.yml

const props = defineProps<{ show: boolean }>()
const section = defineModel<'overview' | 'shortcuts'>('section', { default: 'shortcuts' })
const emit = defineEmits<{ close: [] }>()

const { defaultEditor } = useAppConfig()

// The global key handler stands down while any *-overlay is open, so this
// overlay owns its own keys.
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' || (e.key === '?' && !e.metaKey)) {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    section.value = section.value === 'overview' ? 'shortcuts' : 'overview'
  }
}
watch(
  () => props.show,
  (open) => {
    if (open) window.addEventListener('keydown', onKey, true)
    else window.removeEventListener('keydown', onKey, true)
  },
  { immediate: true },
)
onUnmounted(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <div v-if="show" class="search-overlay help-overlay" @click.self="emit('close')">
    <div class="help-modal" role="dialog" aria-label="Dispatch help">
      <div class="help-tabs" role="tablist">
        <button
          role="tab"
          :aria-selected="section === 'overview'"
          :class="{ active: section === 'overview' }"
          @click="section = 'overview'"
        >
          How Dispatch publishes
        </button>
        <button
          role="tab"
          :aria-selected="section === 'shortcuts'"
          :class="{ active: section === 'shortcuts' }"
          @click="section = 'shortcuts'"
        >
          Keyboard shortcuts
        </button>
      </div>

      <!-- ── How Dispatch publishes ─────────────────────────────────── -->
      <div v-if="section === 'overview'" class="overview">
        <section>
          <div class="help-section-title">
            <PhRocket :size="10" weight="duotone" />
            Publishing
          </div>
          <ol class="steps">
            <li>
              <b>Publish</b> copies the note into website2 and pushes it.
              <span class="sys">commit “Publish: slug” to main, then git push</span>
            </li>
            <li>
              GitHub builds and deploys it, usually in about 3 minutes. The banner says
              <b>DEPLOYING…</b> meanwhile.
              <span class="sys">GitHub Actions</span>
            </li>
            <li>
              When ejfox.com serves that commit, the banner says <b>LIVE ✓</b>. If the deploy fails its
              health check, the site rolls itself back and the banner says <b>NOT LIVE</b>, with a link to
              the run.
            </li>
          </ol>
          <p>
            <b>Republish</b> does the same for a post that's already live. <b>Unpublish</b> takes it off the
            site and deploys again; your vault note isn't touched.
            <span class="sys">moves the site copy to content/drafts/</span>
          </p>
          <p>
            Publishing stops first if website2 isn't on main, or if someone edited the site's copy outside
            Dispatch. Anything in the Lint Receipt also blocks Publish until it's fixed.
          </p>
        </section>

        <section>
          <div class="help-section-title">Never published</div>
          <p>
            Week notes (a <code>week-notes/</code> folder or <code>type: weeknote</code>) and anything in a
            <code>drafts/</code> folder. A note with <code>draft: true</code> asks you to clear it first.
            Sealed posts go out with <code>yarn seal</code>, never through Dispatch.
          </p>
        </section>

        <section>
          <div class="help-section-title">
            <PhEye :size="10" weight="duotone" />
            Who can see it
          </div>
          <div class="visibility-spectrum">
            <div class="vis-row">
              <span class="vis-badge vis-public">
                <PhCheckCircle :size="9" weight="fill" />
                PUBLIC
              </span>
              <span class="vis-desc">In listings, feeds and search</span>
            </div>
            <div class="vis-row">
              <span class="vis-badge vis-unlisted">UNLISTED</span>
              <span class="vis-desc">
                Anyone with the link; kept out of listings and feeds.
                <code>unlisted: true</code>
                (the Unlisted button adds it for you)
              </span>
            </div>
            <div class="vis-row">
              <span class="vis-badge vis-protected">PASSWORD</span>
              <span class="vis-desc">
                Link plus password.
                <code>password: xyz</code>
              </span>
            </div>
          </div>
        </section>

        <section>
          <div class="help-section-title">Publishing later vs. scheduling</div>
          <p>
            <b>Publish later…</b> is Dispatch's own timer: it publishes the post at the time you pick, checking
            every minute, so keep Dispatch running (closing the window only hides it; ⌘Q stops it).
            <span class="sys">publish_at in the note</span>
            <b>Scheduling</b> is the site's embargo: publish now with <code>publishAt</code> (or a future
            <code>date</code>) in the frontmatter, and ejfox.com keeps the post hidden until then, with no
            second deploy. Mind the time zone: <code>publishAt: 2026-10-01</code> means midnight UTC, 8pm the
            evening before in New York, so write the offset:
            <code>publishAt: "2026-10-01T09:00:00-04:00"</code>.
          </p>
        </section>

        <section>
          <div class="help-section-title">After it's live</div>
          <p>
            Once the deploy is confirmed, Dispatch sends <b>webmentions</b> to every site you linked to (and
            to the fediverse via Bridgy Fed, if that's on in Settings). The site's own after-deploy jobs also
            send webmentions and copy the post to your AT Protocol repo.
            <span class="sys">send-webmentions, mirror-atproto</span>
          </p>
          <p>
            Bluesky and Mastodon posts only go out when you use <b>Syndicate</b>. <b>Post now</b> sends
            right away, <b>Queue</b> sends at the times you picked (while Dispatch is open), and
            <b>Dry run</b> sends nothing.
          </p>
        </section>

        <div class="help-hint">
          <button class="link-btn" @click="section = 'shortcuts'">Keyboard shortcuts →</button>
          ·
          <kbd>esc</kbd>
          to close
        </div>
      </div>

      <!-- ── Keyboard shortcuts ─────────────────────────────────────── -->
      <div v-else>
        <div class="help-grid">
          <div class="help-section">
            <div class="help-section-title">
              <PhCursor :size="10" weight="duotone" />
              Post list (Preview tab)
            </div>
            <div class="help-row"><kbd>&uarr;</kbd><kbd>&darr;</kbd> or <kbd>j</kbd><kbd>k</kbd><span>move up/down</span></div>
            <div class="help-row"><kbd>g</kbd><kbd>g</kbd> <kbd>G</kbd><span>top / bottom</span></div>
            <div class="help-row"><kbd>[</kbd><kbd>]</kbd><span>prev/next (wrap)</span></div>
            <div class="help-row"><kbd>1</kbd>–<kbd>9</kbd><span>jump to nth post</span></div>
            <div class="help-row"><kbd>esc</kbd><span>deselect</span></div>
            <div class="help-row"><kbd>o</kbd> or <kbd>&#8984;&#8679;O</kbd><span>open in Obsidian</span></div>
            <div class="help-row"><kbd>i</kbd><span>open in {{ defaultEditor }}</span></div>
            <div class="help-row"><kbd>&#8984;&#8679;R</kbd><span>reveal in Finder</span></div>
            <div class="help-row"><kbd>&#8984;&#8997;C</kbd><span>copy file path</span></div>
            <div class="help-row"><kbd>p</kbd><span>{{ SITE_PREVIEW_LABEL.toLowerCase() }}</span></div>
            <div class="help-row"><kbd>v</kbd><span>view on site</span></div>
            <div class="help-row"><kbd>c</kbd> or <kbd>&#8984;&#8679;L</kbd><span>copy public URL</span></div>
          </div>
          <div class="help-section">
            <div class="help-section-title">
              <PhRocket :size="10" weight="duotone" />
              Publish
            </div>
            <div class="help-row"><kbd>&#8984;&#8629;</kbd><span>publish / republish</span></div>
            <div class="help-row"><kbd>&#8984;&#8679;&#8629;</kbd><span>publish as unlisted</span></div>
            <div class="help-section-title spaced">
              <PhSquaresFour :size="10" weight="duotone" />
              Tabs
            </div>
            <div class="help-row"><kbd>d</kbd><span>Desk</span></div>
            <div class="help-row"><kbd>&#8984;1</kbd><span>Preview</span></div>
            <div class="help-row"><kbd>&#8984;2</kbd> or <kbd>m</kbd><span>Media</span></div>
            <div class="help-row"><kbd>&#8984;3</kbd> or <kbd>J</kbd><span>Journal</span></div>
            <div class="help-row"><kbd>&#8984;4</kbd><span>{{ CHANGED_LABEL }}</span></div>
            <div class="help-row"><kbd>&#8984;5</kbd><span>Activity</span></div>
          </div>
        </div>
        <div class="help-divider"></div>
        <div class="help-grid">
          <div class="help-section">
            <div class="help-section-title">
              <PhKeyboard :size="10" weight="duotone" />
              App
            </div>
            <div class="help-row"><kbd>&#8984;K</kbd> or <kbd>/</kbd><span>search posts + commands</span></div>
            <div class="help-row"><kbd>&#8984;N</kbd> or <kbd>n</kbd><span>new post</span></div>
            <div class="help-row"><kbd>&#8984;R</kbd> or <kbd>r</kbd><span>rescan vault</span></div>
            <div class="help-row"><kbd>&#8984;0</kbd><span>hide/show sidebar</span></div>
            <div class="help-row"><kbd>&#8984;&#8679;C</kbd><span>compact rows</span></div>
            <div class="help-row"><kbd>&#8984;,</kbd><span>settings</span></div>
            <div class="help-row"><kbd>&#8984;/</kbd> or <kbd>?</kbd><span>this help</span></div>
          </div>
          <div class="help-section">
            <div class="help-section-title">
              <PhNewspaper :size="10" weight="duotone" />
              The Desk
            </div>
            <div class="help-row"><kbd>&#8629;</kbd><span>start today's piece</span></div>
            <div class="help-row"><kbd>1</kbd>–<kbd>5</kbd><span>pick an angle (the title)</span></div>
            <div class="help-row"><kbd>s</kbd><span>open the sketch</span></div>
            <div class="help-row"><kbd>x</kbd><span>fact-check my version</span></div>
            <div class="help-row"><kbd>f</kbd><span>open the bench folder</span></div>
          </div>
        </div>
        <div class="help-hint">
          <button class="link-btn" @click="section = 'overview'">How Dispatch publishes →</button>
          ·
          <kbd>?</kbd>
          or
          <kbd>esc</kbd>
          to close
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.help-modal {
  width: 540px;
  max-height: calc(100vh - 120px);
  overflow-y: auto;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  padding: var(--space-5);
  box-shadow: var(--shadow-lg);
  animation: scaleIn 0.25s var(--ease-out-expo);
}

.help-tabs {
  display: flex;
  justify-content: center;
  gap: 4px;
  margin-bottom: 16px;
}

.help-tabs button {
  font-size: 12px;
  font-weight: 600;
  padding: 4px 10px;
  border-radius: 6px;
  color: var(--text-tertiary);
}

.help-tabs button.active {
  color: var(--text-primary);
  background: var(--kbd-bg);
}

.overview section + section {
  margin-top: 14px;
}

.overview p,
.overview li {
  font-size: 11px;
  line-height: 1.55;
  color: var(--text-secondary);
  margin: 0 0 6px;
}

.overview .steps {
  margin: 0 0 6px;
  padding-left: 18px;
}

.overview b {
  color: var(--text-primary);
  font-weight: 600;
}

/* The git/system word for a step, kept quiet: for EJ, not for the reader. */
.sys {
  display: block;
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  color: var(--text-tertiary);
}

.overview code,

.help-section-title.spaced {
  margin-top: 12px;
}

.link-btn {
  font-size: 10px;
  color: var(--text-secondary);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.help-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}

.help-section-title {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
  margin-bottom: 8px;
  display: flex;
  align-items: center;
  gap: 4px;
}

.help-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.help-row span {
  margin-left: auto;
  color: var(--text-tertiary);
}

.help-hint {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
  text-align: center;
  font-size: 10px;
  color: var(--text-tertiary);
}

.help-hint :deep(kbd) {
  font-size: 9px;
  padding: 1px 4px;
}

.help-divider {
  margin: 12px 0;
  border-top: 1px solid var(--border);
}

.visibility-spectrum {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.vis-row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 10px;
}

.vis-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 3px;
  min-width: 80px;
  text-align: center;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 3px;
}

/* Visibility is not a status — all three are neutral tags. */
.vis-public,
.vis-unlisted,
.vis-protected {
  background: var(--fill-2);
  color: var(--label);
}

.vis-desc {
  color: var(--text-tertiary);
}

</style>
