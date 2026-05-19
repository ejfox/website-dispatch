<script setup lang="ts">
import { computed } from 'vue'
import type { Backlink } from '../types'

const props = defineProps<{
  backlinks: Backlink[]
  currentTitle: string
}>()

const emit = defineEmits<{ select: [path: string] }>()

const SIZE = 220
const CENTER = SIZE / 2
const NODE_R = 7
// Radial radius scales gently with node count so 1-2 backlinks don't crowd
// the center and 12+ don't overflow the canvas.
const radius = computed(() => Math.min(85, 40 + props.backlinks.length * 4))

const nodes = computed(() =>
  props.backlinks.map((bl, i) => {
    const n = props.backlinks.length
    // -π/2 starts the first node at 12 o'clock and goes clockwise.
    const angle = (i / Math.max(n, 1)) * Math.PI * 2 - Math.PI / 2
    return {
      ...bl,
      x: CENTER + Math.cos(angle) * radius.value,
      y: CENTER + Math.sin(angle) * radius.value,
    }
  }),
)

// Short label = first three letters of the title (or filename) per node.
const labelFor = (bl: Backlink) => {
  const src = bl.title || bl.path.split('/').pop()?.replace(/\.md$/, '') || ''
  return src.replace(/[^a-zA-Z0-9]/g, '').slice(0, 3).toUpperCase()
}
</script>

<template>
  <div v-if="nodes.length" class="backlinks-graph">
    <svg :width="SIZE" :height="SIZE" class="graph-svg">
      <line
        v-for="node in nodes"
        :key="`edge-${node.path}`"
        :x1="CENTER"
        :y1="CENTER"
        :x2="node.x"
        :y2="node.y"
        class="graph-edge"
      />
      <g
        v-for="node in nodes"
        :key="node.path"
        class="graph-node"
        @click="emit('select', node.path)"
      >
        <circle :cx="node.x" :cy="node.y" :r="NODE_R" />
        <text :x="node.x" :y="node.y + 3" class="graph-node-label">{{ labelFor(node) }}</text>
        <title>{{ node.title || node.path }}</title>
      </g>
      <circle :cx="CENTER" :cy="CENTER" :r="NODE_R + 4" class="graph-center" />
      <text :x="CENTER" :y="CENTER + 3" class="graph-center-label">●</text>
    </svg>
  </div>
</template>

<style scoped>
.backlinks-graph {
  display: flex;
  justify-content: center;
  margin: 6px 0 10px;
}
.graph-svg {
  overflow: visible;
}
.graph-edge {
  stroke: var(--text-tertiary, #555);
  stroke-width: 1;
  opacity: 0.35;
}
.graph-node {
  cursor: pointer;
}
.graph-node circle {
  fill: rgba(107, 26, 61, 0.6);
  stroke: var(--text-secondary, #aaa);
  stroke-width: 1;
  transition: fill 0.15s, r 0.15s;
}
.graph-node:hover circle {
  fill: var(--text-primary);
  stroke: var(--text-primary);
}
.graph-node-label {
  font-size: 8px;
  font-family: 'SF Mono', Menlo, monospace;
  fill: var(--text-secondary, #ccc);
  text-anchor: middle;
  pointer-events: none;
  letter-spacing: 0.5px;
}
.graph-node:hover .graph-node-label {
  fill: var(--bg, #000);
}
.graph-center {
  fill: var(--text-primary, #e5e5e5);
  stroke: rgba(107, 26, 61, 0.8);
  stroke-width: 2;
}
.graph-center-label {
  font-size: 9px;
  fill: var(--bg, #000);
  text-anchor: middle;
  pointer-events: none;
}
</style>
