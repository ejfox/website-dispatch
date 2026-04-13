<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { PhLockSimple, PhEye, PhCheckCircle } from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

const props = defineProps<{
  show: boolean
  files: MarkdownFile[]
}>()

const emit = defineEmits<{
  close: []
  select: [file: MarkdownFile]
}>()

const searchQuery = ref('')
const selectedIndex = ref(0)
const searchInput = ref<HTMLInputElement | null>(null)

const searchResults = computed(() => {
  if (!searchQuery.value.trim()) return props.files.slice(0, 20)
  const q = searchQuery.value.toLowerCase()
  return props.files
    .filter((f) => {
      const title = (f.title || f.filename).toLowerCase()
      const tags = f.tags.join(' ').toLowerCase()
      return title.includes(q) || tags.includes(q) || f.filename.toLowerCase().includes(q)
    })
    .slice(0, 20)
})

watch(
  () => props.show,
  (open) => {
    if (open) {
      searchQuery.value = ''
      selectedIndex.value = 0
      setTimeout(() => searchInput.value?.focus(), 10)
    }
  },
)

function handleSearchKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  } else if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, searchResults.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const result = searchResults.value[selectedIndex.value]
    if (result) emit('select', result)
  }
}

function selectResult(file: MarkdownFile) {
  emit('select', file)
}
</script>

<template>
  <Transition name="modal">
    <div v-if="show" class="search-overlay" @click.self="emit('close')">
      <div class="search-modal">
        <input
          ref="searchInput"
          v-model="searchQuery"
          type="text"
          placeholder="Search posts..."
          class="search-input"
          @keydown="handleSearchKey"
        />
        <div class="search-results">
          <button
            v-for="(file, i) in searchResults"
            :key="file.path"
            class="search-result"
            :class="{ selected: i === selectedIndex }"
            @click="selectResult(file)"
            @mouseenter="selectedIndex = i"
          >
            <span v-if="file.password" class="result-badge protected"><PhLockSimple :size="10" weight="fill" /></span>
            <span v-else-if="file.unlisted" class="result-badge unlisted"><PhEye :size="10" weight="fill" /></span>
            <span v-else-if="file.published_url" class="result-badge live">
              <PhCheckCircle :size="10" weight="fill" />
            </span>
            <span class="result-title">{{ file.title || file.filename.replace('.md', '') }}</span>
            <span class="result-words">{{ file.word_count }}w</span>
            <span class="result-dir">{{ file.source_dir }}</span>
          </button>
          <div v-if="searchResults.length === 0" class="no-results">No results</div>
        </div>
        <div class="search-hint">
          <span>&uarr;&darr; navigate</span>
          <span>&crarr; select</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.search-modal {
  width: 500px;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
}

.search-input {
  width: 100%;
  padding: 14px 16px;
  font-size: 15px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border);
  color: var(--text-primary);
  outline: none;
}

.search-input::placeholder {
  color: var(--text-tertiary);
}

.search-results {
  max-height: 350px;
  overflow-y: auto;
}

.search-result {
  width: 100%;
  padding: 10px 16px;
  display: flex;
  align-items: center;
  gap: 8px;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s ease;
}

.search-result:hover {
  background: var(--accent);
}

.search-result.selected {
  background: var(--selection-bg);
  color: var(--selection-text);
}

.search-result.selected .result-title {
  color: var(--selection-text);
}

.search-result.selected .result-dir {
  color: var(--selection-text);
  opacity: 0.7;
}

.result-badge {
  font-size: 9px;
  padding: 2px 5px;
  border-radius: 3px;
  flex-shrink: 0;
}

.result-badge.live {
  background: var(--success);
  color: #000;
}

.result-badge.unlisted {
  background: #6366f1;
  color: #fff;
}

.result-badge.protected {
  background: #8b5cf6;
  color: #fff;
}

.result-words {
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.result-title {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.result-dir {
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}

.no-results {
  padding: 20px;
  text-align: center;
  color: var(--text-tertiary);
}

.search-hint {
  padding: 8px 16px;
  border-top: 1px solid var(--border);
  display: flex;
  gap: 16px;
  font-size: 10px;
  color: var(--text-tertiary);
}
</style>
