<script setup lang="ts">
import { PhCursor, PhKeyboard, PhEye, PhCheckCircle } from '@phosphor-icons/vue'

defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  close: []
}>()
</script>

<template>
  <div v-if="show" class="search-overlay" @click.self="emit('close')">
    <div class="help-modal">
      <div class="help-title">Keyboard Shortcuts</div>
      <div class="help-grid">
        <div class="help-section">
          <div class="help-section-title">
            <PhCursor :size="10" weight="duotone" />
            Navigation
          </div>
          <div class="help-row">
            <kbd>&uarr;</kbd>
            <kbd>&darr;</kbd>
            or
            <kbd>j</kbd>
            <kbd>k</kbd>
            <span>move up/down</span>
          </div>
          <div class="help-row">
            <kbd>g</kbd>
            <kbd>g</kbd>
            <span>go to top</span>
          </div>
          <div class="help-row">
            <kbd>G</kbd>
            <span>go to bottom</span>
          </div>
          <div class="help-row">
            <kbd>[</kbd>
            <kbd>]</kbd>
            <span>prev/next (wrap)</span>
          </div>
          <div class="help-row">
            <kbd>1</kbd>
            -
            <kbd>9</kbd>
            <span>jump to nth file</span>
          </div>
          <div class="help-row">
            <kbd>esc</kbd>
            <span>deselect</span>
          </div>
        </div>
        <div class="help-section">
          <div class="help-section-title">
            <PhKeyboard :size="10" weight="duotone" />
            Actions
          </div>
          <div class="help-row">
            <kbd>&#8984;</kbd>
            <kbd>K</kbd>
            or
            <kbd>/</kbd>
            <span>search</span>
          </div>
          <div class="help-row">
            <kbd>o</kbd>
            <span>open in Obsidian</span>
          </div>
          <div class="help-row">
            <kbd>i</kbd>
            <span>open in iA Writer</span>
          </div>
          <div class="help-row">
            <kbd>p</kbd>
            <span>preview</span>
          </div>
          <div class="help-row">
            <kbd>v</kbd>
            <span>view on site</span>
          </div>
          <div class="help-row">
            <kbd>c</kbd>
            <span>copy URL</span>
          </div>
          <div class="help-row">
            <kbd>n</kbd>
            <span>new post</span>
          </div>
          <div class="help-row">
            <kbd>r</kbd>
            <span>refresh</span>
          </div>
          <div class="help-row">
            <kbd>m</kbd>
            <span>media library</span>
          </div>
          <div class="help-row">
            <kbd>&#8984;</kbd>
            <kbd>&#8629;</kbd>
            <span>publish</span>
          </div>
          <div class="help-row">
            <kbd>,</kbd>
            <span>settings</span>
          </div>
        </div>
      </div>
      <div class="help-divider"></div>
      <div class="help-section-title">
        <PhEye :size="10" weight="duotone" />
        Visibility Spectrum
      </div>
      <div class="visibility-spectrum">
        <div class="vis-row">
          <span class="vis-badge vis-public">
            <PhCheckCircle :size="9" weight="fill" />
            PUBLIC
          </span>
          <span class="vis-desc">Appears in listings, feeds, search</span>
        </div>
        <div class="vis-row">
          <span class="vis-badge vis-unlisted">UNLISTED</span>
          <span class="vis-desc">
            Link only — add
            <code>unlisted: true</code>
          </span>
        </div>
        <div class="vis-row">
          <span class="vis-badge vis-protected">PROTECTED</span>
          <span class="vis-desc">
            Link + password — add
            <code>password: xyz</code>
          </span>
        </div>
      </div>
      <div class="help-hint">
        Press
        <kbd>?</kbd>
        or
        <kbd>esc</kbd>
        to close
      </div>
    </div>
  </div>
</template>

<style scoped>
.help-modal {
  width: 480px;
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

.help-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 16px;
  text-align: center;
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

.vis-public {
  background: var(--success);
  color: #000;
}

.vis-unlisted {
  background: #6366f1;
  color: #fff;
}

.vis-protected {
  background: #8b5cf6;
  color: #fff;
}

.vis-desc {
  color: var(--text-tertiary);
}

.vis-desc code {
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  background: var(--kbd-bg);
  padding: 1px 4px;
  border-radius: 2px;
}
</style>
