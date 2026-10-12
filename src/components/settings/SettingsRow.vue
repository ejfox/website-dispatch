<script setup lang="ts">
/**
 * One row of a Settings pane: a right-aligned label, the control(s), then one
 * plain sentence saying what the setting does. Validation notes go in the
 * `note` slot, under the help. Lives inside a `.st-form` grid.
 */
defineProps<{ label?: string; help?: string; for?: string }>()
</script>

<template>
  <label class="st-label" :for="$props.for">{{ label }}</label>
  <div class="st-control">
    <slot />
    <p v-if="help" class="st-help">{{ help }}</p>
    <slot name="note" />
  </div>
</template>
