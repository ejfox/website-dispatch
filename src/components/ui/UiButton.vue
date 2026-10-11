<script setup lang="ts">
/**
 * The one button. Every clickable push-button in Dispatch is one of these.
 *
 *   variant   primary      AccentColor fill, AccentColorText. ONE per screen, max.
 *             secondary    neutral hairline, --fill on hover. The default.
 *             plain        borderless; toolbars, icon buttons, modal "Cancel".
 *             destructive  secondary shape, red text + glyph. Never a red fill.
 *   size      regular (28px) · small (22px) — macOS NSControl sizes, one radius each.
 *   icon      a Phosphor component; always 14px bold. Icon-only buttons MUST
 *             carry aria-label (dev warns).
 *   menu      trailing ▾ + aria-haspopup, for buttons that pop a native menu.
 *   href      renders an <a target=_blank> with the same look.
 *   block     flex: 1 — equal segments inside a UiButtonGroup or a filled row.
 *
 * Labels never wrap: they truncate with an ellipsis and get a title tooltip
 * (unless a `data-tip` HUD tooltip is already on the element).
 */
import { computed, useAttrs, useSlots, watchEffect, type Component } from 'vue'
import { PhCaretDown } from '@phosphor-icons/vue'

defineOptions({ inheritAttrs: false })

const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'plain' | 'destructive'
    size?: 'regular' | 'small'
    icon?: Component
    label?: string
    href?: string
    menu?: boolean
    block?: boolean
    disabled?: boolean
    type?: 'button' | 'submit'
  }>(),
  { variant: 'secondary', size: 'regular', menu: false, block: false, disabled: false, type: 'button' },
)

const emit = defineEmits<{ click: [e: MouseEvent] }>()

const attrs = useAttrs()
const slots = useSlots()

const hasLabel = computed(() => !!props.label || !!slots.default)
const iconOnly = computed(() => !hasLabel.value && !!props.icon)

// Native title doubles as the truncation tooltip. Skip it when the caller
// already attached a `data-tip` HUD tooltip so the two don't stack.
const title = computed<string | undefined>(() => {
  if (attrs.title !== undefined) return attrs.title as string
  if ('data-tip' in attrs) return undefined
  if (iconOnly.value) return attrs['aria-label'] as string | undefined
  return props.label
})

if (import.meta.env.DEV) {
  watchEffect(() => {
    if (iconOnly.value && !attrs['aria-label']) {
      console.warn('[UiButton] icon-only button is missing aria-label', props.icon)
    }
  })
}

function onClick(e: MouseEvent) {
  if (props.disabled) {
    e.preventDefault()
    return
  }
  emit('click', e)
}
</script>

<template>
  <component
    :is="href ? 'a' : 'button'"
    v-bind="attrs"
    class="ui-btn"
    :class="[
      `is-${variant}`,
      `is-${size}`,
      { 'is-icon-only': iconOnly, 'is-block': block, 'has-menu': menu },
    ]"
    :type="href ? undefined : type"
    :href="href"
    :target="href ? '_blank' : undefined"
    :rel="href ? 'noopener' : undefined"
    :disabled="href ? undefined : disabled || undefined"
    :aria-disabled="href && disabled ? 'true' : undefined"
    :aria-haspopup="menu ? 'menu' : undefined"
    :title="title"
    @click="onClick"
  >
    <component :is="icon" v-if="icon" :size="14" weight="bold" class="ui-btn-icon" aria-hidden="true" />
    <span v-if="hasLabel" class="ui-btn-label"><slot>{{ label }}</slot></span>
    <PhCaretDown v-if="menu" :size="10" weight="bold" class="ui-btn-caret" aria-hidden="true" />
  </component>
</template>

<style scoped>
.ui-btn {
  --btn-h: 28px;
  --btn-px: var(--space-3);
  --btn-r: 6px;
  --btn-fs: 12px;

  appearance: none;
  -webkit-appearance: none;
  box-sizing: border-box;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: var(--btn-h);
  min-width: var(--btn-h);
  max-width: 100%;
  padding: 0 var(--btn-px);
  margin: 0;
  font: inherit;
  font-size: var(--btn-fs);
  font-weight: 500;
  line-height: 1;
  color: var(--label);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--btn-r);
  cursor: default;
  user-select: none;
  -webkit-user-select: none;
  text-decoration: none;
  white-space: nowrap;
  -webkit-app-region: no-drag;
  transition:
    background 0.12s ease,
    color 0.12s ease,
    border-color 0.12s ease;
}
.ui-btn.is-small {
  --btn-h: 22px;
  --btn-px: var(--space-2);
  --btn-r: 5px;
  --btn-fs: 11px;
}
.ui-btn.is-block {
  flex: 1 1 0;
}
.ui-btn.is-icon-only {
  width: var(--btn-h);
  padding: 0;
}

.ui-btn-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.ui-btn-icon,
.ui-btn-caret {
  flex-shrink: 0;
}
.ui-btn-caret {
  opacity: 0.6;
  margin-left: -2px;
}

/* ── secondary (default): neutral hairline, fill on hover ── */
.ui-btn.is-secondary {
  border-color: var(--separator);
}
.ui-btn.is-secondary:hover:not(:disabled):not([aria-disabled='true']) {
  background: var(--fill);
}
.ui-btn.is-secondary:active:not(:disabled):not([aria-disabled='true']) {
  background: var(--fill-2);
}

/* ── primary: the one accent action on the screen ── */
.ui-btn.is-primary {
  background: var(--accent);
  color: var(--on-accent);
  font-weight: 600;
}
.ui-btn.is-primary:hover:not(:disabled):not([aria-disabled='true']) {
  background: var(--accent-strong);
}
.ui-btn.is-primary:active:not(:disabled):not([aria-disabled='true']) {
  /* macOS push-buttons darken on press; they don't move. */
  filter: brightness(0.92);
}

/* ── plain: borderless, for toolbars, icon buttons, modal "Cancel" ── */
.ui-btn.is-plain {
  color: var(--label-2);
}
.ui-btn.is-plain:hover:not(:disabled):not([aria-disabled='true']) {
  background: var(--fill);
  color: var(--label);
}
.ui-btn.is-plain:active:not(:disabled):not([aria-disabled='true']) {
  background: var(--fill-2);
}

/* ── destructive: secondary shape, red word + glyph, never a red fill.
   The glyph takes the system red; the word takes --ink-broken, the same
   hue deepened in light mode so it clears 4.5:1 (see global.css). ── */
.ui-btn.is-destructive {
  border-color: var(--separator);
  color: var(--ink-broken);
}
.ui-btn.is-destructive .ui-btn-icon {
  color: var(--status-broken);
}
.ui-btn.is-destructive:hover:not(:disabled):not([aria-disabled='true']) {
  background: var(--tint-broken);
}
.ui-btn.is-destructive:active:not(:disabled):not([aria-disabled='true']) {
  background: color-mix(in srgb, var(--status-broken) 24%, transparent);
}

/* ── states ── */
.ui-btn:disabled,
.ui-btn[aria-disabled='true'] {
  opacity: 0.45;
}
.ui-btn:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px var(--accent-focus);
}

/* Keyboard hint inside a label, e.g. Publish ⌘↩. Inherits the button's
   ink so it reads on an accent fill too. */
.ui-btn :deep(kbd) {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 1px 4px;
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  line-height: 1.2;
  color: inherit;
  background: color-mix(in srgb, currentColor 18%, transparent);
  border: none;
  border-radius: 3px;
  opacity: 0.85;
}
</style>
