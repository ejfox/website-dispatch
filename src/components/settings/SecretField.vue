<script setup lang="ts">
/**
 * A field for a stored secret. The backend only ever sends "••••" + the last
 * four characters, so that's all this can show. Focusing a masked field
 * selects it; typing replaces it, leaving it alone keeps the stored value,
 * and emptying it clears the secret. New input is typed into a password
 * field and committed on Return or blur.
 */
import { computed, ref } from 'vue'
import { isMasked } from '../../composables/useSettings'

const props = defineProps<{ modelValue: string; id?: string; placeholder?: string; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [v: string]; commit: [] }>()

const editing = ref(false)
const masked = computed(() => isMasked(props.modelValue))

function onFocus(e: FocusEvent) {
  editing.value = true
  if (masked.value) (e.target as HTMLInputElement).select()
}
function onBlur() {
  editing.value = false
}
</script>

<template>
  <input
    :id="id"
    class="st-input is-mono"
    :type="editing && !masked ? 'password' : 'text'"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    autocomplete="off"
    autocorrect="off"
    autocapitalize="off"
    spellcheck="false"
    @focus="onFocus"
    @blur="onBlur"
    @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    @change="emit('commit')"
  />
</template>
