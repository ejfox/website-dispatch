<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  PhArrowSquareUpRight,
  PhFolderOpen,
  PhImageSquare,
  PhMagnifyingGlass,
  PhPencilSimple,
} from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

const props = defineProps<{ files: MarkdownFile[] }>()
const emit = defineEmits<{
  'select-file': [file: MarkdownFile]
}>()

type ProjectFilter = 'all' | 'live' | 'changed' | 'drafts'

const filter = ref<ProjectFilter>('all')
const query = ref('')
const sort = ref<'recent' | 'title' | 'date'>('recent')

const projects = computed(() => props.files.filter((file) => file.content_type === 'project'))
const liveCount = computed(() => projects.value.filter((file) => file.published_url).length)
const changedCount = computed(
  () => projects.value.filter((file) => file.warnings.includes('Modified since publish')).length,
)
const draftCount = computed(() => projects.value.filter((file) => !file.published_url).length)

const visibleProjects = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase()
  const visible = projects.value.filter((file) => {
    if (filter.value === 'live' && !file.published_url) return false
    if (filter.value === 'changed' && !file.warnings.includes('Modified since publish')) return false
    if (filter.value === 'drafts' && file.published_url) return false
    if (!needle) return true
    return [file.title, file.filename, file.dek, ...file.tags]
      .filter(Boolean)
      .some((value) => String(value).toLocaleLowerCase().includes(needle))
  })

  return [...visible].sort((a, b) => {
    if (sort.value === 'title') {
      return displayTitle(a).localeCompare(displayTitle(b), undefined, { sensitivity: 'base' })
    }
    if (sort.value === 'date') {
      return (Date.parse(b.date || '') || 0) - (Date.parse(a.date || '') || 0)
    }
    return b.modified - a.modified
  })
})

function displayTitle(file: MarkdownFile): string {
  return file.title || file.filename.replace(/\.md$/, '')
}

function projectSlug(file: MarkdownFile): string {
  return `/projects/${file.filename.replace(/\.md$/, '')}`
}

function projectYear(file: MarkdownFile): string | null {
  if (!file.date) return null
  const year = file.date.match(/\b(19|20)\d{2}\b/)?.[0]
  return year || null
}

function statusFor(file: MarkdownFile): { label: string; tone: string } {
  if (file.warnings.includes('Modified since publish')) return { label: 'Changed', tone: 'changed' }
  if (file.published_url) return { label: 'Live', tone: 'live' }
  if (file.draft) return { label: 'Draft', tone: 'draft' }
  return { label: 'Not published', tone: 'unpublished' }
}

function thumbnailUrl(url: string): string {
  if (url.includes('res.cloudinary.com') && url.includes('/upload/')) {
    return url.replace('/upload/', '/upload/w_800,h_450,c_fill,q_auto,f_auto/')
  }
  return url
}

function formatAge(ts: number): string {
  const seconds = Math.max(0, Math.floor(Date.now() / 1000 - ts))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days === 1) return 'yesterday'
  if (days < 30) return `${days}d ago`
  if (days < 365) return `${Math.floor(days / 30)}mo ago`
  return `${Math.floor(days / 365)}y ago`
}
</script>

<template>
  <div class="projects-view">
    <header class="projects-header">
      <div class="heading">
        <h2>Projects</h2>
        <p>
          {{ projects.length }} portfolio entr{{ projects.length === 1 ? 'y' : 'ies' }}
          <span aria-hidden="true">·</span> {{ liveCount }} live
          <template v-if="changedCount">
            <span aria-hidden="true">·</span> {{ changedCount }} changed
          </template>
        </p>
      </div>

      <label class="project-search">
        <PhMagnifyingGlass :size="13" />
        <input v-model="query" type="search" placeholder="Search projects" aria-label="Search projects" />
      </label>
      <select v-model="sort" class="sort-select" aria-label="Sort projects" data-no-drag>
        <option value="recent">Recently edited</option>
        <option value="date">Project date</option>
        <option value="title">Title</option>
      </select>
    </header>

    <nav class="project-filters" aria-label="Project filters">
      <button :class="{ active: filter === 'all' }" @click="filter = 'all'">All <span>{{ projects.length }}</span></button>
      <button :class="{ active: filter === 'live' }" @click="filter = 'live'">Live <span>{{ liveCount }}</span></button>
      <button :class="{ active: filter === 'changed' }" @click="filter = 'changed'">Changed <span>{{ changedCount }}</span></button>
      <button :class="{ active: filter === 'drafts' }" @click="filter = 'drafts'">Not published <span>{{ draftCount }}</span></button>
    </nav>

    <div v-if="projects.length === 0" class="empty-state">
      <PhFolderOpen :size="36" weight="duotone" />
      <strong>No portfolio projects found</strong>
      <span>Markdown files in <code>blog/projects/</code> will appear here.</span>
    </div>

    <div v-else-if="visibleProjects.length === 0" class="empty-state">
      <PhMagnifyingGlass :size="32" />
      <strong>No matching projects</strong>
      <button @click="query = ''; filter = 'all'">Clear filters</button>
    </div>

    <div v-else class="project-grid">
      <article
        v-for="file in visibleProjects"
        :key="file.path"
        class="project-card"
        tabindex="0"
        @click="emit('select-file', file)"
        @keydown.enter="emit('select-file', file)"
        @keydown.space.prevent="emit('select-file', file)"
      >
        <div class="project-art">
          <img v-if="file.thumbnail" :src="thumbnailUrl(file.thumbnail)" :alt="`${displayTitle(file)} preview`" loading="lazy" />
          <div v-else class="art-placeholder">
            <PhImageSquare :size="32" weight="duotone" />
            <span>PROJECT</span>
          </div>
          <span class="status-pill" :class="statusFor(file).tone">{{ statusFor(file).label }}</span>
          <a
            v-if="file.published_url"
            class="live-link"
            :href="file.published_url"
            target="_blank"
            rel="noopener noreferrer"
            aria-label="Open live project"
            data-tip="Open live project"
            @click.stop
          >
            <PhArrowSquareUpRight :size="14" />
          </a>
        </div>

        <div class="project-copy">
          <div class="project-kicker">
            <span>{{ projectYear(file) || 'PROJECT' }}</span>
            <span>{{ file.word_count.toLocaleString() }}w</span>
            <span v-if="file.image_count">{{ file.image_count }} image{{ file.image_count === 1 ? '' : 's' }}</span>
          </div>
          <h3>{{ displayTitle(file) }}</h3>
          <p v-if="file.dek" class="project-dek">{{ file.dek }}</p>
          <p v-else class="project-path">{{ projectSlug(file) }}</p>
          <div class="project-footer">
            <div class="project-tags">
              <span v-for="tag in file.tags.slice(0, 3)" :key="tag">{{ tag }}</span>
            </div>
            <span class="edited"><PhPencilSimple :size="10" /> {{ formatAge(file.modified) }}</span>
          </div>
        </div>
      </article>
    </div>
  </div>
</template>

<style scoped>
.projects-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  color: var(--text-primary);
}

.projects-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px 10px;
}
.heading { flex: 1; min-width: 150px; }
.heading h2 { margin: 0; font-size: 15px; font-weight: 650; }
.heading p { margin: 3px 0 0; color: var(--text-tertiary); font-size: 10px; }
.heading p span { margin: 0 3px; }

.project-search {
  width: min(220px, 30%);
  height: 28px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 9px;
  background: var(--fill);
  border: 1px solid transparent;
  border-radius: 7px;
  color: var(--text-tertiary);
}
.project-search:focus-within { border-color: var(--accent-focus); }
.project-search input {
  min-width: 0;
  width: 100%;
  padding: 0;
  border: 0;
  outline: 0;
  background: transparent;
  color: var(--text-primary);
  font: inherit;
  font-size: 11px;
}
.project-search input::placeholder { color: var(--text-tertiary); }

.sort-select {
  height: 28px;
  padding: 0 24px 0 9px;
  border: 1px solid var(--border-light);
  border-radius: 7px;
  background-color: var(--bg-solid);
  color: var(--text-secondary);
  font: inherit;
  font-size: 10px;
}

.project-filters {
  display: flex;
  gap: 4px;
  padding: 0 16px 10px;
  border-bottom: 1px solid var(--border);
}
.project-filters button {
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-tertiary);
  padding: 4px 8px;
  font: inherit;
  font-size: 10px;
  cursor: pointer;
}
.project-filters button:hover { background: var(--fill); color: var(--text-secondary); }
.project-filters button.active { background: var(--accent-tint); color: var(--text-primary); }
.project-filters button span { margin-left: 3px; font-variant-numeric: tabular-nums; opacity: 0.65; }

.project-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 12px;
  padding: 14px 16px 24px;
  overflow-y: auto;
}
.project-card {
  min-width: 0;
  overflow: hidden;
  background: color-mix(in srgb, var(--bg-tertiary) 55%, transparent);
  border: 1px solid var(--border-light);
  border-radius: 10px;
  cursor: pointer;
  transition: border-color 120ms ease, background 120ms ease, transform 120ms ease;
}
.project-card:hover {
  background: var(--bg-tertiary);
  border-color: color-mix(in srgb, var(--accent) 45%, var(--border-light));
  transform: translateY(-1px);
}
.project-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

.project-art {
  position: relative;
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border-bottom: 1px solid var(--border);
  background: var(--bg-primary);
}
.project-art img { width: 100%; height: 100%; object-fit: cover; display: block; }
.art-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  color: var(--label-3);
  background:
    linear-gradient(135deg, transparent 48%, var(--fill) 49%, var(--fill) 51%, transparent 52%),
    var(--bg-primary);
  background-size: 28px 28px;
}
.art-placeholder span { font-size: 9px; letter-spacing: 0.16em; font-weight: 700; }

.status-pill {
  position: absolute;
  left: 8px;
  top: 8px;
  padding: 3px 6px;
  border-radius: 5px;
  backdrop-filter: blur(10px);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.status-pill.live { color: var(--status-live); background: color-mix(in srgb, var(--bg-solid) 82%, var(--status-live)); }
.status-pill.changed { color: var(--status-attention); background: color-mix(in srgb, var(--bg-solid) 82%, var(--status-attention)); }
.status-pill.draft,
.status-pill.unpublished { color: var(--text-secondary); background: color-mix(in srgb, var(--bg-solid) 86%, transparent); }
.live-link {
  position: absolute;
  right: 8px;
  top: 8px;
  width: 25px;
  height: 25px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  background: color-mix(in srgb, var(--bg-solid) 86%, transparent);
  color: var(--text-primary);
}
.live-link:hover { background: var(--accent); color: var(--accent-contrast); }

.project-copy { padding: 10px 11px 11px; }
.project-kicker {
  display: flex;
  gap: 7px;
  color: var(--text-tertiary);
  font-size: 9px;
  font-variant-numeric: tabular-nums;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.project-copy h3 {
  margin: 5px 0 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 600;
}
.project-dek,
.project-path {
  display: -webkit-box;
  min-height: 28px;
  margin: 5px 0 8px;
  overflow: hidden;
  color: var(--text-secondary);
  font-size: 10px;
  line-height: 1.4;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}
.project-path { color: var(--text-tertiary); font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.project-footer { display: flex; align-items: center; gap: 8px; min-height: 18px; }
.project-tags { display: flex; gap: 4px; min-width: 0; flex: 1; overflow: hidden; }
.project-tags span {
  flex-shrink: 0;
  max-width: 90px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding: 2px 5px;
  border-radius: 4px;
  background: var(--fill);
  color: var(--text-tertiary);
  font-size: 8px;
}
.edited { display: flex; align-items: center; gap: 3px; flex-shrink: 0; color: var(--text-tertiary); font-size: 9px; }

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  text-align: center;
}
.empty-state strong { color: var(--text-secondary); font-size: 13px; }
.empty-state span { font-size: 10px; }
.empty-state code { font-size: 10px; }
.empty-state button { border: 0; background: transparent; color: var(--accent); cursor: pointer; font: inherit; }

@media (max-width: 720px) {
  .projects-header { flex-wrap: wrap; }
  .project-search { width: 100%; order: 3; }
  .project-grid { grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); }
}
</style>
