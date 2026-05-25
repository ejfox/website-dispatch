<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhX, PhCaretDown, PhArrowFatLineUp, PhArrowFatLineDown } from '@phosphor-icons/vue'
import type { PublishDiff } from '../types'

const props = defineProps<{
  filePath: string
  /** Parent controls open/closed so the "See changes" toggle in StatusBanner
   *  can flip the same state both directions. */
  open: boolean
}>()

const emit = defineEmits<{ close: [] }>()

const diff = ref<PublishDiff | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

async function fetchDiff() {
  loading.value = true
  error.value = null
  try {
    diff.value = (await invoke('get_publish_diff', { filePath: props.filePath })) as PublishDiff
  } catch (e: any) {
    error.value = typeof e === 'string' ? e : (e?.message ?? String(e))
  } finally {
    loading.value = false
  }
}

// Lazy-load: only fetch when the panel first opens, and refetch if the file changes.
watch(
  () => [props.open, props.filePath] as const,
  ([open, _]) => {
    if (open) fetchDiff()
  },
  { immediate: true },
)
</script>

<template>
  <div v-if="open" class="diff-panel">
    <div class="diff-header">
      <span class="diff-title">Changes since last publish</span>
      <template v-if="diff && diff.has_diff">
        <span class="diff-stat added" :data-tip="`${diff.words_added} words added across ${diff.lines_added} lines`">
          <PhArrowFatLineUp :size="9" weight="bold" />
          +{{ diff.words_added }}<span class="diff-stat-unit">w</span>
        </span>
        <span
          class="diff-stat removed"
          :data-tip="`${diff.words_removed} words removed across ${diff.lines_removed} lines`"
        >
          <PhArrowFatLineDown :size="9" weight="bold" />
          −{{ diff.words_removed }}<span class="diff-stat-unit">w</span>
        </span>
      </template>
      <button class="diff-close" @click="emit('close')" data-tip="Collapse">
        <PhX :size="11" weight="bold" />
      </button>
    </div>

    <div v-if="loading" class="diff-state muted">Computing diff…</div>

    <div v-else-if="error" class="diff-state error">{{ error }}</div>

    <div v-else-if="diff && !diff.has_diff && !diff.error" class="diff-state muted">
      No textual changes — only whitespace or frontmatter differs.
    </div>

    <div v-else-if="diff && diff.error" class="diff-state muted">{{ diff.error }}</div>

    <div v-else-if="diff" class="diff-hunks">
      <div v-for="(hunk, hi) in diff.hunks" :key="hi" class="diff-hunk">
        <div class="hunk-header">
          <PhCaretDown :size="8" weight="bold" />
          <span class="hunk-range"
            >line {{ hunk.source_start }}{{ hunk.lines.length > 1 ? `–${hunk.source_start + hunk.lines.length - 1}` : '' }}</span
          >
        </div>
        <div class="hunk-body">
          <div v-for="(line, li) in hunk.lines" :key="li" class="diff-line" :class="line.tag">
            <span class="ln source">{{ line.source_line ?? '' }}</span>
            <span class="ln gutter">
              <template v-if="line.tag === 'added'">+</template>
              <template v-else-if="line.tag === 'removed'">−</template>
              <template v-else>&nbsp;</template>
            </span>
            <span class="diff-content">{{ line.content || ' ' }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.diff-panel {
  border-bottom: 1px solid var(--border);
  background: var(--bg-tertiary);
  animation: diff-slide-down 0.18s ease-out;
}

@keyframes diff-slide-down {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.diff-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-secondary);
}

.diff-title {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-secondary);
}

.diff-stat {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 3px;
  font-variant-numeric: tabular-nums;
}
.diff-stat.added {
  color: var(--success);
  background: color-mix(in srgb, var(--success) 14%, transparent);
}
.diff-stat.removed {
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 14%, transparent);
}
.diff-stat-unit {
  opacity: 0.55;
  margin-left: 1px;
}

.diff-close {
  margin-left: auto;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 3px;
  border-radius: 3px;
  display: inline-flex;
}
.diff-close:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}

.diff-state {
  padding: 10px 16px;
  font-size: 11px;
}
.diff-state.muted {
  color: var(--text-tertiary);
}
.diff-state.error {
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 12%, transparent);
  border-top: 1px solid color-mix(in srgb, var(--danger) 25%, transparent);
}

.diff-hunks {
  max-height: 360px;
  overflow-y: auto;
  font-family: 'SF Mono', monospace;
  font-size: 11px;
  line-height: 1.45;
}

.diff-hunk + .diff-hunk {
  border-top: 1px solid var(--border);
}

.hunk-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 16px;
  background: var(--bg-tertiary);
  font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  font-size: 10px;
  color: var(--text-tertiary);
}
.hunk-range {
  font-family: 'SF Mono', monospace;
  font-variant-numeric: tabular-nums;
}

.hunk-body {
  padding: 4px 0;
}

.diff-line {
  display: grid;
  grid-template-columns: 36px 14px 1fr;
  align-items: baseline;
  padding: 0 16px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.diff-line.added {
  background: color-mix(in srgb, var(--success) 10%, transparent);
  color: var(--text-primary);
}
.diff-line.removed {
  background: color-mix(in srgb, var(--danger) 10%, transparent);
  color: var(--text-primary);
}
.diff-line.equal {
  color: var(--text-tertiary);
}

.ln {
  text-align: right;
  font-variant-numeric: tabular-nums;
  user-select: none;
}
.ln.source {
  padding-left: 16px;
  color: var(--text-tertiary);
  font-size: 10px;
}
.ln.gutter {
  font-weight: 700;
  text-align: center;
}
.diff-line.added .ln.gutter {
  color: var(--success);
}
.diff-line.removed .ln.gutter {
  color: var(--danger);
}

.diff-content {
  padding-left: 8px;
}
</style>
