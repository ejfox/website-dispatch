<script setup lang="ts">
/**
 * The inspector's "Add an image" block. Always on screen under Publish, so
 * the way images get into a post is never something you have to discover.
 * Drops land anywhere on the window (App.vue); this shows how, and what
 * happened last.
 */
import { computed, watch } from 'vue'
import { PhImageSquare, PhCheckCircle, PhWarningCircle } from '@phosphor-icons/vue'
import UiButton from './ui/UiButton.vue'
import { useImageUpload } from '../composables/useImageUpload'
import { useAppConfig } from '../composables/useVaultData'
import type { MarkdownFile } from '../types'

const props = defineProps<{ file: MarkdownFile }>()

const { state, chooseAndUpload, copyAgain, reset } = useImageUpload()
const { defaultEditor } = useAppConfig()

// "Press ⌘V" stops being true once the paste lands (the note changes on disk)
// or you've moved to another post.
watch([() => props.file.path, () => props.file.modified], reset)

const uploadingLabel = computed(() => {
  if (state.value.phase !== 'uploading') return ''
  return state.value.count === 1 ? 'Uploading…' : `Uploading ${state.value.count} files…`
})
</script>

<template>
  <section class="insp-section image-well" :data-phase="state.phase" aria-live="polite">
    <span class="insp-label">Add an image</span>

    <template v-if="state.phase === 'ready'">
      <p class="well-line">
        <PhCheckCircle :size="16" weight="fill" class="well-icon ok" />
        <span>Copied. Press <kbd>⌘V</kbd> in {{ defaultEditor }}.</span>
      </p>
      <span v-if="state.needsAlt" class="insp-note">It has no alt text yet. Describe below can write some.</span>
      <div class="well-actions">
        <UiButton size="small" label="Copy again" @click="copyAgain" />
        <UiButton size="small" variant="plain" label="Add another…" @click="chooseAndUpload(props.file)" />
      </div>
    </template>

    <p v-else-if="state.phase === 'uploading'" class="well-line">
      <PhImageSquare :size="16" class="well-icon" />
      <span>{{ uploadingLabel }}</span>
    </p>

    <template v-else>
      <p v-if="state.phase === 'failed'" class="well-line">
        <PhWarningCircle :size="16" weight="fill" class="well-icon bad" />
        <span>{{ state.message }}</span>
      </p>
      <div class="well-drop">
        <PhImageSquare :size="20" class="well-icon" />
        <span>Drag a screenshot anywhere onto this window</span>
        <UiButton size="small" label="Choose…" @click="chooseAndUpload(props.file)" />
      </div>
      <span class="insp-note">It uploads, {{ defaultEditor }} comes forward, and you press ⌘V.</span>
    </template>
  </section>
</template>

<style scoped>
.well-drop {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px dashed var(--label-3);
  border-radius: 8px;
  font-size: 12px;
  color: var(--label-2);
  text-align: center;
}
.well-line {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--label);
  overflow-wrap: anywhere;
}
.well-icon {
  flex-shrink: 0;
  color: var(--label-2);
}
.well-icon.ok {
  color: var(--status-live);
}
.well-icon.bad {
  color: var(--status-broken);
}
.well-actions {
  display: flex;
  gap: var(--space-2);
}
</style>
