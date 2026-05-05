<script setup lang="ts">
import { useToasts } from '../composables/useToasts'
import { PhCheckCircle, PhInfo, PhWarning, PhWarningOctagon, PhX } from '@phosphor-icons/vue'

const { toasts, dismiss } = useToasts()

const iconFor = {
  success: PhCheckCircle,
  info: PhInfo,
  warn: PhWarning,
  error: PhWarningOctagon,
} as const
</script>

<template>
  <Teleport to="body">
    <div class="toast-stack" role="status" aria-live="polite">
      <TransitionGroup name="toast">
        <div v-for="t in toasts" :key="t.id" class="toast" :class="`kind-${t.kind}`" :data-tip="t.detail || undefined">
          <component :is="iconFor[t.kind]" :size="16" weight="fill" class="toast-icon" />
          <div class="toast-body">
            <div class="toast-message">{{ t.message }}</div>
            <div v-if="t.detail" class="toast-detail">{{ t.detail }}</div>
          </div>
          <button
            v-if="t.action"
            class="toast-action"
            @click="
              () => {
                t.action!.run()
              }
            "
          >
            {{ t.action.label }}
          </button>
          <button class="toast-dismiss" @click="dismiss(t.id)" aria-label="Dismiss">
            <PhX :size="12" weight="bold" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column-reverse;
  gap: 8px;
  z-index: 9999;
  pointer-events: none;
  max-width: min(540px, calc(100vw - 48px));
  width: max-content;
}

.toast {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: color-mix(in srgb, var(--bg-solid) 88%, transparent);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 10px;
  box-shadow:
    0 8px 24px rgba(0, 0, 0, 0.18),
    0 0 0 0.5px rgba(0, 0, 0, 0.08);
  font-size: 13px;
  color: var(--text-primary);
  min-width: 280px;
}

.toast.kind-success .toast-icon {
  color: var(--success);
}
.toast.kind-info .toast-icon {
  color: var(--text-secondary);
}
.toast.kind-warn .toast-icon {
  color: var(--warning);
}
.toast.kind-error .toast-icon {
  color: var(--danger);
}

.toast-icon {
  flex: none;
}

.toast-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.toast-message {
  font-weight: 500;
  line-height: 1.3;
}

.toast-detail {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: 'SF Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-action,
.toast-dismiss {
  background: transparent;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-weight: 500;
  transition: all var(--transition-fast);
  flex: none;
}

.toast-action {
  padding: 4px 10px;
}

.toast-action:hover {
  background: var(--accent);
  color: var(--text-primary);
}

.toast-dismiss {
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  opacity: 0.6;
}

.toast-dismiss:hover {
  opacity: 1;
  background: var(--accent);
}

/* TransitionGroup animations — slide up from the bottom, fade out. */
.toast-enter-active,
.toast-leave-active {
  transition: all 220ms cubic-bezier(0.16, 1, 0.3, 1);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(12px) scale(0.96);
}

.toast-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}

.toast-move {
  transition: transform 220ms cubic-bezier(0.16, 1, 0.3, 1);
}
</style>
