<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhBackpack, PhMapPin, PhClock, PhCube, PhFloppyDisk, PhArrowsClockwise, PhWarning } from '@phosphor-icons/vue'

interface Gear {
  name: string
  weight_oz: string
  parent_container: string
  type: string
  category: string
  subcategory: string
  priority: string
  waterproof: string
  worn: string
  qty: string
  consumable: string
  star: string
  notes: string
  tags: string
  condition: string
  amazon_url: string
  last_used: string
  purchase_date: string
  purchase_price: string
  photo_url: string
  scan_3d_url: string
  serial_number: string
  model_number: string
  brand: string
  location_room: string
  location_detail: string
}

interface Pending {
  dirty: boolean
  diff_stat: string
}

const items = ref<Gear[]>([])
const filter = ref('')
const cursor = ref(0)
const loading = ref(false)
const error = ref<string | null>(null)
const pending = ref<Pending>({ dirty: false, diff_stat: '' })
const status = ref<string | null>(null)

// inline editor state
const editing = ref<null | 'location' | 'scan'>(null)
const editRoom = ref('')
const editDetail = ref('')
const editScan = ref('')
const editInput = ref<HTMLInputElement | null>(null)

const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase()
  if (!q) return items.value
  return items.value.filter((it) => {
    const hay =
      `${it.name} ${it.type} ${it.parent_container} ${it.location_room} ${it.location_detail} ${it.tags}`.toLowerCase()
    return hay.includes(q)
  })
})

const selected = computed(() => filtered.value[cursor.value] ?? null)

const today = () => new Date().toISOString().slice(0, 10)
const isStale = (last: string) => {
  if (!last) return true
  const d = new Date(last)
  if (Number.isNaN(d.getTime())) return true
  const days = (Date.now() - d.getTime()) / 86400000
  return days > 90
}

async function load() {
  loading.value = true
  error.value = null
  try {
    items.value = await invoke<Gear[]>('list_gear')
    if (cursor.value >= filtered.value.length) cursor.value = 0
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
  await refreshPending()
}

async function refreshPending() {
  try {
    pending.value = await invoke<Pending>('gear_pending_changes')
  } catch (e) {
    pending.value = { dirty: false, diff_stat: '' }
  }
}

function flash(msg: string) {
  status.value = msg
  setTimeout(() => {
    if (status.value === msg) status.value = null
  }, 2000)
}

async function markUsed() {
  if (!selected.value) return
  const name = selected.value.name
  try {
    await invoke('mark_gear_used', { names: [name] })
    flash(`stamped ${today()} on ${name}`)
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

function startLocationEdit() {
  if (!selected.value) return
  editing.value = 'location'
  editRoom.value = selected.value.location_room || ''
  editDetail.value = selected.value.location_detail || ''
  nextTick(() => editInput.value?.focus())
}

async function saveLocation() {
  if (!selected.value) return
  try {
    await invoke('update_gear_location', {
      name: selected.value.name,
      room: editRoom.value,
      detail: editDetail.value,
    })
    flash(`location updated`)
    editing.value = null
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

function startScanEdit() {
  if (!selected.value) return
  editing.value = 'scan'
  editScan.value = selected.value.scan_3d_url || ''
  nextTick(() => editInput.value?.focus())
}

async function saveScan() {
  if (!selected.value) return
  try {
    await invoke('set_gear_scan_url', {
      name: selected.value.name,
      url: editScan.value,
    })
    flash(`scan url saved`)
    editing.value = null
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

async function commitChanges() {
  try {
    const msg = await invoke<string>('commit_gear_changes', { message: null })
    flash(msg)
    await refreshPending()
  } catch (e: any) {
    error.value = String(e)
  }
}

async function openInTui() {
  // shell to gear-tui via Tauri shell plugin if available; otherwise just notify
  try {
    const shell = await import('@tauri-apps/plugin-shell')
    const cmd = shell.Command.create('gear-tui', [])
    await cmd.spawn()
    flash('launched gear-tui')
  } catch (e) {
    flash('open ~/.local/bin/gear-tui yourself')
  }
}

function onKey(e: KeyboardEvent) {
  // bail if a modifier is involved or focus is in an input we own
  if (e.metaKey || e.ctrlKey || e.altKey) return
  const t = e.target as HTMLElement
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) {
    if (e.key === 'Escape') {
      editing.value = null
      ;(t as HTMLInputElement).blur()
      e.preventDefault()
    }
    return
  }

  switch (e.key) {
    case 'j':
      cursor.value = Math.min(cursor.value + 1, filtered.value.length - 1)
      e.preventDefault()
      break
    case 'k':
      cursor.value = Math.max(cursor.value - 1, 0)
      e.preventDefault()
      break
    case 'g':
      cursor.value = 0
      e.preventDefault()
      break
    case 'G':
      cursor.value = Math.max(filtered.value.length - 1, 0)
      e.preventDefault()
      break
    case 'u':
      markUsed()
      e.preventDefault()
      break
    case 'l':
      startLocationEdit()
      e.preventDefault()
      break
    case 's':
      startScanEdit()
      e.preventDefault()
      break
    case 'e':
      openInTui()
      e.preventDefault()
      break
    case 'c':
      commitChanges()
      e.preventDefault()
      break
    case 'r':
      load()
      e.preventDefault()
      break
    case '/': {
      const el = document.querySelector<HTMLInputElement>('#gear-filter')
      if (el) {
        el.focus()
        e.preventDefault()
      }
      break
    }
  }
}

onMounted(() => {
  load()
  window.addEventListener('keydown', onKey)
})
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <div class="gear-panel">
    <div class="gear-header">
      <div class="gear-title">
        <PhBackpack :size="14" weight="duotone" />
        <span>gear</span>
        <span class="muted">· {{ filtered.length }}/{{ items.length }}</span>
      </div>
      <div class="gear-actions">
        <span v-if="status" class="flash">{{ status }}</span>
        <span v-if="pending.dirty" class="pending-badge" :title="pending.diff_stat">
          <PhWarning :size="11" />
          uncommitted
        </span>
        <button class="hdr-btn" @click="load" :disabled="loading" title="refresh (r)">
          <PhArrowsClockwise :size="12" :class="{ spin: loading }" />
        </button>
        <button class="hdr-btn" @click="commitChanges" :disabled="!pending.dirty" title="commit (c)">
          <PhFloppyDisk :size="12" />
        </button>
      </div>
    </div>

    <input
      id="gear-filter"
      v-model="filter"
      class="gear-filter"
      placeholder="filter…  (/ to focus)"
      @keydown.escape="
        (e) => {
          ;(e.target as HTMLInputElement).blur()
        }
      "
    />

    <div v-if="error" class="gear-error">{{ error }}</div>

    <div class="gear-list" tabindex="0">
      <div
        v-for="(it, i) in filtered"
        :key="it.name"
        class="gear-row"
        :class="{ active: i === cursor }"
        @click="cursor = i"
      >
        <span class="gear-name">{{ it.name }}</span>
        <span class="gear-meta">
          <span v-if="it.location_room" class="loc">
            <PhMapPin :size="9" />
            {{ it.location_room }}
          </span>
          <span class="last" :class="{ stale: isStale(it.last_used) }">
            <PhClock :size="9" />
            {{ it.last_used || '—' }}
          </span>
          <span v-if="it.scan_3d_url" class="scan" title="has 3D scan"><PhCube :size="9" /></span>
        </span>
      </div>
      <div v-if="!filtered.length && !loading" class="empty-row">no items match</div>
    </div>

    <div v-if="selected" class="gear-detail">
      <div class="detail-name">{{ selected.name }}</div>
      <div class="detail-grid">
        <div>
          <span class="lbl">type</span>
          {{ selected.type || '—' }}
        </div>
        <div>
          <span class="lbl">weight</span>
          {{ selected.weight_oz || '—' }} oz
        </div>
        <div>
          <span class="lbl">in</span>
          {{ selected.parent_container || '—' }}
        </div>
        <div>
          <span class="lbl">last used</span>
          {{ selected.last_used || '—' }}
        </div>
        <div class="span2">
          <span class="lbl">location</span>
          {{ selected.location_room || '—' }}{{ selected.location_detail ? ' / ' + selected.location_detail : '' }}
        </div>
        <div class="span2">
          <span class="lbl">scan</span>
          {{ selected.scan_3d_url || '—' }}
        </div>
      </div>

      <div v-if="editing === 'location'" class="inline-edit">
        <input
          ref="editInput"
          v-model="editRoom"
          placeholder="Location_Room"
          @keydown.enter="saveLocation"
          @keydown.escape="editing = null"
        />
        <input
          v-model="editDetail"
          placeholder="Location_Detail"
          @keydown.enter="saveLocation"
          @keydown.escape="editing = null"
        />
        <button @click="saveLocation">save</button>
      </div>
      <div v-else-if="editing === 'scan'" class="inline-edit">
        <input
          ref="editInput"
          v-model="editScan"
          placeholder="Scan_3D_URL"
          @keydown.enter="saveScan"
          @keydown.escape="editing = null"
        />
        <button @click="saveScan">save</button>
      </div>

      <div class="shortcuts">
        <kbd>u</kbd>
        mark used
        <kbd>l</kbd>
        location
        <kbd>s</kbd>
        scan url
        <kbd>e</kbd>
        open tui
        <kbd>c</kbd>
        commit
        <kbd>j/k</kbd>
        nav
      </div>
    </div>
  </div>
</template>

<style scoped>
.gear-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  font-size: 12px;
  color: var(--text, #ddd);
}

.gear-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border, #222);
}

.gear-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
}

.muted {
  color: var(--muted, #888);
  font-weight: 400;
}

.gear-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.flash {
  color: var(--accent, #6eedf7);
  font-size: 11px;
}

.pending-badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  color: #e6a000;
  border: 1px solid #735865;
  border-radius: 3px;
  padding: 1px 5px;
}

.hdr-btn {
  background: transparent;
  border: 1px solid var(--border, #222);
  color: inherit;
  border-radius: 3px;
  padding: 3px 5px;
  cursor: pointer;
}
.hdr-btn:disabled {
  opacity: 0.4;
  cursor: default;
}
.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.gear-filter {
  margin: 8px 10px;
  background: var(--bg-alt, #0d0d0d);
  color: inherit;
  border: 1px solid var(--border, #222);
  border-radius: 3px;
  padding: 5px 8px;
  font-size: 12px;
  font-family: inherit;
}

.gear-error {
  margin: 0 10px 6px;
  color: #e60067;
  font-size: 11px;
}

.gear-list {
  flex: 1;
  overflow-y: auto;
  border-top: 1px solid var(--border, #222);
}

.gear-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 10px;
  cursor: pointer;
  border-bottom: 1px solid #161616;
}

.gear-row.active {
  background: var(--accent-bg, #6b1a3d);
  color: #fff;
}

.gear-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
}

.gear-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 10px;
  color: var(--muted, #888);
}

.gear-row.active .gear-meta {
  color: #ffd0e0;
}

.gear-meta .loc,
.gear-meta .last,
.gear-meta .scan {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.gear-meta .last.stale {
  color: #b86a00;
}

.empty-row {
  padding: 14px 10px;
  color: var(--muted, #666);
  text-align: center;
}

.gear-detail {
  border-top: 1px solid var(--border, #222);
  padding: 8px 10px;
  font-size: 11px;
}

.detail-name {
  font-weight: 600;
  font-size: 12px;
  margin-bottom: 4px;
}

.detail-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px 12px;
}

.detail-grid .span2 {
  grid-column: span 2;
}

.lbl {
  color: var(--muted, #888);
  margin-right: 5px;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.inline-edit {
  margin-top: 6px;
  display: flex;
  gap: 4px;
}

.inline-edit input {
  flex: 1;
  background: var(--bg-alt, #0d0d0d);
  color: inherit;
  border: 1px solid var(--border, #222);
  border-radius: 3px;
  padding: 3px 6px;
  font-size: 11px;
  font-family: inherit;
}

.inline-edit button {
  background: var(--accent, #6eedf7);
  color: #000;
  border: 0;
  border-radius: 3px;
  padding: 0 8px;
  font-size: 11px;
  cursor: pointer;
}

.shortcuts {
  margin-top: 8px;
  font-size: 10px;
  color: var(--muted, #666);
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.shortcuts kbd {
  font-family: inherit;
  background: var(--bg-alt, #161616);
  border: 1px solid var(--border, #222);
  border-radius: 2px;
  padding: 0 4px;
  margin-right: 3px;
  font-size: 10px;
}
</style>
