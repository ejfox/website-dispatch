<script setup lang="ts">
/**
 * Joined buttons for one job ("Open in: Obsidian · iA Writer · Site preview").
 * Children are UiButtons; the group removes the inner radii and overlaps the
 * hairlines so they read as one segmented control. `block` makes the segments
 * share the row's width equally.
 */
withDefaults(defineProps<{ label?: string; block?: boolean }>(), { block: false })
</script>

<template>
  <div class="ui-btn-group" :class="{ 'is-block': block }" role="group" :aria-label="label">
    <slot />
  </div>
</template>

<style scoped>
.ui-btn-group {
  display: inline-flex;
  align-items: stretch;
  max-width: 100%;
  min-width: 0;
}
.ui-btn-group.is-block {
  display: flex;
  width: 100%;
}
.ui-btn-group.is-block :deep(.ui-btn) {
  flex: 1 1 0;
}
.ui-btn-group :deep(.ui-btn) {
  border-radius: 0;
}
.ui-btn-group :deep(.ui-btn:not(:first-child)) {
  margin-left: -1px;
}
.ui-btn-group :deep(.ui-btn:first-child) {
  border-top-left-radius: var(--btn-r);
  border-bottom-left-radius: var(--btn-r);
}
.ui-btn-group :deep(.ui-btn:last-child) {
  border-top-right-radius: var(--btn-r);
  border-bottom-right-radius: var(--btn-r);
}
/* Lift the hovered / focused segment over its neighbours' hairlines. */
.ui-btn-group :deep(.ui-btn:hover),
.ui-btn-group :deep(.ui-btn:focus-visible) {
  position: relative;
  z-index: 1;
}
</style>
