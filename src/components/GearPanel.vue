<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useLocalStorage } from '@vueuse/core'
import { Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu'
import ResizeHandle from './ResizeHandle.vue'
import { useResizable } from '../composables/useResizable'
import {
  PhBackpack,
  PhMapPin,
  PhClock,
  PhCube,
  PhFloppyDisk,
  PhArrowsClockwise,
  PhWarning,
  PhStar,
  PhCheck,
  PhX,
  PhPencilSimple,
  PhArrowSquareOut,
  PhCamera,
  PhDrop,
  PhPackage,
  PhMagnifyingGlass,
} from '@phosphor-icons/vue'

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
const cursor = useLocalStorage('dispatch-gear-cursor', 0)
const loading = ref(false)
const error = ref<string | null>(null)
const pending = ref<Pending>({ dirty: false, diff_stat: '' })
const status = ref<string | null>(null)

// Detail-pane height. `invert: true` because the handle sits on top of the
// detail (which is anchored to the bottom of the column) — dragging up should
// grow it, not shrink it. Same localStorage key as before for continuity.
const {
  size: detailHeight,
  dragging: detailDragging,
  start: startResize,
  reset: resetDetailHeight,
} = useResizable('dispatch-gear-detail-height', {
  default: 360,
  min: 160,
  max: () => window.innerHeight - 180,
  axis: 'y',
  invert: true,
})

// Generic inline-edit state. `editField` is the snake_case field key matching
// the backend's whitelist. `editValue` mirrors the current input.
const editField = ref<string | null>(null)
const editValue = ref('')
// Separate state for the two-field location editor.
const editingLocation = ref(false)
const editRoom = ref('')
const editDetail = ref('')
const editInput = ref<HTMLInputElement | HTMLTextAreaElement | null>(null)

type SortKey = 'name' | 'weight' | 'type' | 'container' | 'location' | 'last_used'
const sortKey = useLocalStorage<SortKey>('dispatch-gear-sort', 'name')
const sortDir = useLocalStorage<'asc' | 'desc'>('dispatch-gear-sort-dir', 'asc')
const containerFilter = useLocalStorage<string>('dispatch-gear-container', '')

const containerOptions = computed(() => {
  const map = new Map<string, number>()
  for (const it of items.value) {
    const k = (it.parent_container || '').trim()
    if (!k) continue
    map.set(k, (map.get(k) || 0) + 1)
  }
  return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]))
})

function toggleSort(key: SortKey) {
  if (sortKey.value === key) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortKey.value = key
    sortDir.value = 'asc'
  }
}

// Used to suppress the most-common last_used date in the row view, but
// once "used" became its own column hiding values made the column look
// broken — show real data instead. Kept removed; restore from git if a
// future "compact" mode wants the de-duping behavior back.

const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase()
  const cont = containerFilter.value.trim()
  let out = items.value
  if (cont) out = out.filter((it) => (it.parent_container || '') === cont)
  if (q) {
    out = out.filter((it) => {
      const hay =
        `${it.name} ${it.brand} ${it.type} ${it.category} ${it.parent_container} ${it.location_room} ${it.location_detail} ${it.tags} ${it.notes}`.toLowerCase()
      return hay.includes(q)
    })
  }
  const dir = sortDir.value === 'asc' ? 1 : -1
  const cmp = (a: string, b: string) => a.localeCompare(b) * dir
  out = [...out].sort((a, b) => {
    switch (sortKey.value) {
      case 'name':
        return cmp(a.name, b.name)
      case 'weight':
        return ((parseFloat(a.weight_oz) || 0) - (parseFloat(b.weight_oz) || 0)) * dir
      case 'type':
        return cmp(a.type || '', b.type || '')
      case 'container':
        return cmp(a.parent_container || '', b.parent_container || '')
      case 'location':
        return cmp(a.location_room || '', b.location_room || '')
      case 'last_used': {
        const av = a.last_used || ''
        const bv = b.last_used || ''
        if (!av && !bv) return 0
        if (!av) return 1
        if (!bv) return -1
        return cmp(av, bv)
      }
    }
  })
  return out
})

const selected = computed(() => filtered.value[cursor.value] ?? null)

// Cancel any in-progress edit when the selected row changes — otherwise the
// editor would silently write back to a different item.
watch(selected, () => {
  editField.value = null
  editingLocation.value = false
})

const today = () => new Date().toISOString().slice(0, 10)
const isStale = (last: string) => {
  if (!last) return true
  const d = new Date(last)
  if (Number.isNaN(d.getTime())) return true
  const days = (Date.now() - d.getTime()) / 86400000
  return days > 90
}
const isTruthy = (v: string) => /^(y|yes|true|1|x|✓)$/i.test((v || '').trim())

// Type → icon mapping. Returns a phosphor component, defaulting to PhPackage.
const typeIcon = (t: string) => {
  const k = (t || '').toLowerCase()
  if (k.includes('camera') || k.includes('photo')) return PhCamera
  if (k.includes('water') || k.includes('hydration') || k.includes('drink')) return PhDrop
  if (k.includes('sleep') || k.includes('bag')) return PhBackpack
  return PhPackage
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

// Helper used by both context menu and detail-panel handlers — sets a single
// gear field on the backend and refreshes the list.
async function setGearField(name: string, field: string, value: string, msg?: string) {
  await invoke('update_gear_field', { name, field, value })
  if (msg) flash(msg)
  await load()
}

const ARCHIVED_CONTAINER = 'archived'

// Shared menu-item factory — used by the right-click submenu *and* the `m`
// keyboard shortcut, so both surfaces stay in sync.
async function buildMoveMenuItems(it: Gear): Promise<(MenuItem | PredefinedMenuItem)[]> {
  const out: (MenuItem | PredefinedMenuItem)[] = []
  for (const [name] of containerOptions.value) {
    if (name === it.parent_container) continue
    out.push(
      await MenuItem.new({
        text: name,
        action: () => setGearField(it.name, 'parent_container', name, `moved to ${name}`),
      }),
    )
  }
  if (it.parent_container) {
    out.push(await PredefinedMenuItem.new({ item: 'Separator' }))
    out.push(
      await MenuItem.new({
        text: 'Remove from container',
        action: () => setGearField(it.name, 'parent_container', '', 'removed from container'),
      }),
    )
  }
  out.push(await PredefinedMenuItem.new({ item: 'Separator' }))
  out.push(
    await MenuItem.new({
      text: 'New container…',
      action: async () => {
        const v = window.prompt('New container name:')?.trim()
        if (v) await setGearField(it.name, 'parent_container', v, `moved to ${v}`)
      },
    }),
  )
  return out
}

// "Got a new one (today)" — stamps a replacement event onto the item.
// Bumps purchase_date to today and condition back to "new". Doesn't touch
// last_used (use `u` separately if you're putting the new one into service).
async function markReplaced() {
  if (!selected.value) return
  const name = selected.value.name
  const t = today()
  try {
    await invoke('update_gear_field', { name, field: 'purchase_date', value: t })
    await invoke('update_gear_field', { name, field: 'condition', value: 'new' })
    flash(`got a new ${name} (${t})`)
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

// Pop the move menu as a standalone (used by the `m` keyboard shortcut).
async function popMoveMenu() {
  if (!selected.value) return
  const items = await buildMoveMenuItems(selected.value)
  const menu = await Menu.new({ items })
  await menu.popup()
}

async function showRowContextMenu(it: Gear, idx: number, e: MouseEvent) {
  e.preventDefault()
  cursor.value = idx
  const isArchived = (it.parent_container || '').toLowerCase() === ARCHIVED_CONTAINER

  const moveSubmenu = await Submenu.new({
    text: 'Move to Container  (m)',
    items: await buildMoveMenuItems(it),
  })

  const items_: (MenuItem | PredefinedMenuItem | Submenu)[] = [
    await MenuItem.new({
      text: `Mark "${it.name}" as Used Today  (u)`,
      action: async () => {
        await invoke('mark_gear_used', { names: [it.name] })
        flash('marked used')
        await load()
      },
    }),
    await MenuItem.new({
      text: 'Got a new one (today)  (R)',
      action: async () => {
        const t = today()
        await invoke('update_gear_field', { name: it.name, field: 'purchase_date', value: t })
        await invoke('update_gear_field', { name: it.name, field: 'condition', value: 'new' })
        flash(`got a new ${it.name} (${t})`)
        await load()
      },
    }),
    await PredefinedMenuItem.new({ item: 'Separator' }),
    moveSubmenu,
    await MenuItem.new({
      text: isArchived ? 'Unarchive' : 'Archive',
      action: () =>
        setGearField(
          it.name,
          'parent_container',
          isArchived ? '' : ARCHIVED_CONTAINER,
          isArchived ? 'unarchived' : 'archived',
        ),
    }),
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({
      text: 'Copy Name',
      action: () => navigator.clipboard.writeText(it.name),
    }),
  ]
  if (it.amazon_url) {
    items_.push(
      await MenuItem.new({
        text: 'View on Amazon',
        action: () => window.open(it.amazon_url, '_blank'),
      }),
    )
  }
  if (it.scan_3d_url) {
    items_.push(
      await MenuItem.new({
        text: 'View 3D Scan',
        action: () => window.open(it.scan_3d_url, '_blank'),
      }),
    )
  }
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
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

function beginEdit(field: string, current: string) {
  if (!selected.value) return
  editingLocation.value = false
  editField.value = field
  editValue.value = current || ''
  nextTick(() => editInput.value?.focus())
}

async function saveField() {
  if (!selected.value || !editField.value) return
  const name = selected.value.name
  const field = editField.value
  const value = editValue.value
  try {
    await invoke('update_gear_field', { name, field, value })
    flash(`saved ${field}`)
    editField.value = null
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

function beginLocationEdit() {
  if (!selected.value) return
  editField.value = null
  editingLocation.value = true
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
    flash('location updated')
    editingLocation.value = false
    await load()
  } catch (e: any) {
    error.value = String(e)
  }
}

async function toggleStar() {
  if (!selected.value) return
  const cur = isTruthy(selected.value.star)
  try {
    await invoke('update_gear_field', {
      name: selected.value.name,
      field: 'star',
      value: cur ? '' : 'true',
    })
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

function openUrl(url: string) {
  if (!url) return
  window.open(url, '_blank')
}

// Build a search query from whatever identifying fields are present.
// brand + model_number is most precise; falls back to name alone.
function searchQuery(it: typeof selected.value): string {
  if (!it) return ''
  const parts = [it.brand, it.model_number, it.name].filter(Boolean) as string[]
  return parts.join(' ').trim()
}

// "Find" menu — pops a native Tauri menu of pre-populated search providers
// so EJ can hunt down a buy link, a review, or a spec sheet without typing
// the item name into every site. Selected providers route to openUrl, which
// opens in the system browser.
async function showFindMenu() {
  if (!selected.value) return
  const q = searchQuery(selected.value)
  if (!q) {
    flash('no name to search for')
    return
  }
  const e = encodeURIComponent(q)
  const providers: Array<[string, string]> = [
    ['Google', `https://www.google.com/search?q=${e}`],
    ['Google Shopping', `https://www.google.com/search?tbm=shop&q=${e}`],
    ['Google Images', `https://www.google.com/search?tbm=isch&q=${e}`],
    ['Amazon', `https://www.amazon.com/s?k=${e}`],
    ['eBay', `https://www.ebay.com/sch/i.html?_nkw=${e}`],
    ['REI', `https://www.rei.com/search?q=${e}`],
    ['Backcountry', `https://www.backcountry.com/Store/catalog/results.jsp?s=u&q=${e}`],
    ['B&H Photo', `https://www.bhphotovideo.com/c/search?q=${e}`],
    ['YouTube (reviews)', `https://www.youtube.com/results?search_query=${e}+review`],
    ['DuckDuckGo', `https://duckduckgo.com/?q=${e}`],
  ]
  const items = await Promise.all(
    providers.map(([label, url]) =>
      MenuItem.new({ text: label, action: () => openUrl(url) }),
    ),
  )
  // Quality-of-life extra: copy the search query itself.
  const sep = await PredefinedMenuItem.new({ item: 'Separator' })
  const copy = await MenuItem.new({
    text: `Copy query  “${q.length > 28 ? q.slice(0, 28) + '…' : q}”`,
    action: () => {
      navigator.clipboard.writeText(q).then(() => flash('copied query'))
    },
  })
  const menu = await Menu.new({ items: [...items, sep, copy] })
  await menu.popup()
}

async function openInTui() {
  try {
    const shell = await import('@tauri-apps/plugin-shell')
    const cmd = shell.Command.create('gear-tui', [])
    await cmd.spawn()
    flash('launched gear-tui')
  } catch (e) {
    flash('open ~/.local/bin/gear-tui yourself')
  }
}

// Helper: gear consumes the key and prevents the global handler from also
// running (we're registered with capture:true). Used inside onKey for any
// key gear claims as its own.
function consume(e: KeyboardEvent) {
  e.preventDefault()
  e.stopImmediatePropagation()
}

function onKey(e: KeyboardEvent) {
  if (e.metaKey || e.ctrlKey || e.altKey) return
  const t = e.target as HTMLElement
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) {
    if (e.key === 'Escape') {
      editField.value = null
      editingLocation.value = false
      ;(t as HTMLInputElement).blur()
      e.preventDefault()
      return
    }
    // Let ArrowUp/ArrowDown fall through to the navigation switch below
    // even while #gear-filter is focused — up/down in a single-line
    // input does nothing useful, and the user expects to filter-type
    // then arrow-down into the list without an extra Tab or click.
    // j/k stay blocked because they're real characters in a filter
    // string ("jacket", "knife", etc).
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') {
      return
    }
  }

  switch (e.key) {
    case 'j':
    case 'ArrowDown':
      cursor.value = Math.min(cursor.value + 1, filtered.value.length - 1)
      consume(e)
      break
    case 'k':
    case 'ArrowUp':
      cursor.value = Math.max(cursor.value - 1, 0)
      consume(e)
      break
    case 'g':
      cursor.value = 0
      consume(e)
      break
    case 'G':
      cursor.value = Math.max(filtered.value.length - 1, 0)
      consume(e)
      break
    case 'u':
      markUsed()
      consume(e)
      break
    case 'l':
      beginLocationEdit()
      consume(e)
      break
    case 's':
      if (selected.value) beginEdit('scan_3d_url', selected.value.scan_3d_url)
      consume(e)
      break
    case 'n':
      // gear's `n` = edit notes. Stops the global handler from also
      // opening the New Post modal.
      if (selected.value) beginEdit('notes', selected.value.notes)
      consume(e)
      break
    case '*':
      toggleStar()
      consume(e)
      break
    case 'e':
      openInTui()
      consume(e)
      break
    case 'c':
      // gear's `c` = commit. Stops the global file-action `c` (copy URL).
      commitChanges()
      consume(e)
      break
    case 'r':
      // gear's `r` = reload gear. Stops the global `r` (refresh files).
      load()
      consume(e)
      break
    case 'm':
      // Move-to-container picker. Without consume() the global handler
      // would also fire `m` and toggle to the Media tab at the same
      // time — opening the move menu *and* switching panes.
      popMoveMenu()
      consume(e)
      break
    case 'R':
      // "Got a new one of that" — bumps purchase_date + condition.
      markReplaced()
      consume(e)
      break
    case '/': {
      // gear's `/` focuses the gear filter. Stops global `/` from
      // opening the search modal at the same time.
      const el = document.querySelector<HTMLInputElement>('#gear-filter')
      if (el) {
        el.focus()
        consume(e)
      }
      break
    }
  }
}

const tagsList = computed(() => {
  if (!selected.value?.tags) return []
  return selected.value.tags
    .split(/[,;]/)
    .map((s) => s.trim())
    .filter(Boolean)
})

onMounted(() => {
  load()
  // Capture phase so gear consumes its keys BEFORE the global
  // useKeyboardShortcuts handler runs. Without this, gear's `m` for
  // Move would fire AND the global `m` toggle-media handler would also
  // fire — opening the move menu and switching tab at the same time.
  // Gear stops propagation on any key it consumes (see onKey).
  window.addEventListener('keydown', onKey, true)
  nextTick(() => {
    document.querySelector<HTMLInputElement>('#gear-filter')?.focus()
  })
})
onUnmounted(() => {
  window.removeEventListener('keydown', onKey, true)
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
      @keydown.escape="(e) => (e.target as HTMLInputElement).blur()"
    />

    <div v-if="error" class="gear-error">{{ error }}</div>

    <div class="gear-controls">
      <!-- Container filter lives on its own row so the sort header below
           gets the full panel width — otherwise the dropdown ate enough
           horizontal budget to overlap the USED column at narrow widths. -->
      <div class="gear-filter-row">
        <select v-model="containerFilter" class="container-select" title="Filter by container">
          <option value="">all containers</option>
          <option v-for="[name, n] in containerOptions" :key="name" :value="name">
            {{ name }} ({{ n }})
          </option>
        </select>
      </div>
      <!-- Sort row doubles as a real column header — uses the same
           --gear-cols grid template as .gear-row so headers line up over
           their values. Every column is clickable; the active one shows a
           direction chevron, inactives reserve the same width so layout
           doesn't jitter when the user changes sort. -->
      <div class="sort-row" role="row">
        <button
          v-for="k in (['name', 'weight', 'type', 'container', 'location', 'last_used'] as SortKey[])"
          :key="k"
          :class="['col-' + (k === 'last_used' ? 'used' : k), { active: sortKey === k }]"
          :title="`Sort by ${k === 'last_used' ? 'last used' : k}`"
          @click="toggleSort(k)"
        >
          <span class="hdr-label">{{ k === 'last_used' ? 'used' : k }}</span>
          <span class="dir" aria-hidden="true">{{ sortKey === k ? (sortDir === 'asc' ? '↑' : '↓') : '' }}</span>
        </button>
      </div>
    </div>

    <div class="gear-list" tabindex="0">
      <div
        v-for="(it, i) in filtered"
        :key="it.name"
        class="gear-row"
        :class="{ active: i === cursor }"
        @click="cursor = i"
        @contextmenu="showRowContextMenu(it, i, $event)"
      >
        <!-- Each cell is a direct grid child, lined up under the matching
             column header. Empty cells render as just a blank slot so the
             columns stay aligned across rows. -->
        <span class="col-name">
          <component :is="typeIcon(it.type || it.category)" :size="12" class="row-type-icon" />
          <PhStar v-if="isTruthy(it.star)" :size="10" weight="fill" class="row-star" />
          <span class="gear-name">{{ it.name }}</span>
          <PhCube v-if="it.scan_3d_url" :size="10" class="scan-icon" title="has 3D scan" />
        </span>
        <span class="col-weight">
          <template v-if="parseFloat(it.weight_oz) > 0">{{ it.weight_oz }}oz</template>
        </span>
        <span class="col-type">
          <span v-if="it.type" class="type-chip">{{ it.type }}</span>
        </span>
        <span class="col-container">
          <!-- Click the chip to filter the list to that container; click again
               to clear. Always visible so you can use it to clear the filter
               from any visible row. -->
          <button
            v-if="it.parent_container"
            class="container-chip"
            :class="{ active: containerFilter === it.parent_container }"
            :title="containerFilter === it.parent_container ? 'clear container filter' : `filter to ${it.parent_container}`"
            @click.stop="containerFilter = (containerFilter === it.parent_container ? '' : it.parent_container)"
          >
            {{ it.parent_container }}
          </button>
          <!-- No container? Fall back to location so the column reads
               consistently even for top-level bags. Muted to signal it's a
               fallback, not an actual container. -->
          <span v-else-if="it.location_room" class="container-fallback" :title="`location: ${it.location_room}`">
            {{ it.location_room }}
          </span>
        </span>
        <span class="col-location">
          <!-- Location and container describe different things (which room
               vs. which bag), so show location whenever it exists — the
               old `&& !it.parent_container` guard was only relevant when
               both crammed into one meta strip. -->
          <template v-if="it.location_room">
            <PhMapPin :size="9" />
            {{ it.location_room }}
          </template>
        </span>
        <span
          class="col-used"
          :class="{ stale: it.last_used && isStale(it.last_used) }"
          :title="it.last_used ? `last used ${it.last_used}` : ''"
        >
          <!-- Show every last_used. The bulkLastUsed suppression made sense
               when this lived in the meta strip (kept noise down). In a
               dedicated column, hiding values makes the column look broken. -->
          <template v-if="it.last_used">
            <PhClock :size="9" />
            {{ it.last_used }}
          </template>
        </span>
      </div>
      <div v-if="!filtered.length && !loading" class="empty-row">no items match</div>
    </div>

    <ResizeHandle
      v-if="selected"
      axis="y"
      :active="detailDragging"
      data-tip="drag to resize · double-click to reset"
      @down="startResize"
      @reset="resetDetailHeight"
    />

    <div v-if="selected" class="gear-detail" :style="{ height: detailHeight + 'px' }">
      <div class="detail-card">
        <div class="detail-photo" :class="{ empty: !selected.photo_url }">
          <img v-if="selected.photo_url" :src="selected.photo_url" :alt="selected.name" />
          <button v-else class="photo-add" @click="beginEdit('photo_url', '')" title="add photo URL">
            <PhCamera :size="22" />
            <span>add photo</span>
          </button>
        </div>

        <div class="detail-body">
          <div class="detail-headline">
            <button class="star-btn" @click="toggleStar" :title="isTruthy(selected.star) ? 'unstar' : 'star'">
              <PhStar v-if="isTruthy(selected.star)" :size="14" weight="fill" class="starred" />
              <PhStar v-else :size="14" />
            </button>
            <div class="detail-name-block">
              <div class="detail-name">{{ selected.name }}</div>
              <div class="detail-sub">
                <template v-if="selected.brand || selected.model_number">
                  {{ [selected.brand, selected.model_number].filter(Boolean).join(' · ') }}
                </template>
                <template v-else>
                  <span class="muted">no brand / model</span>
                </template>
              </div>
            </div>
            <div class="headline-actions">
              <!-- Native menu of search providers, pre-filled with
                   `brand + model + name`. Lets you hunt down a buy link,
                   spec sheet, or review without re-typing into each site. -->
              <button
                class="link-btn"
                @click="showFindMenu"
                data-tip="Search Amazon / Google / eBay / REI / …"
              >
                <PhMagnifyingGlass :size="11" /> find
              </button>
              <button v-if="selected.amazon_url" class="link-btn" @click="openUrl(selected.amazon_url)" title="Amazon">
                <PhArrowSquareOut :size="11" /> amazon
              </button>
              <button v-if="selected.scan_3d_url" class="link-btn" @click="openUrl(selected.scan_3d_url)" title="3D scan">
                <PhCube :size="11" /> scan
              </button>
              <button class="link-btn primary" @click="markUsed" title="mark used today (u)">
                <PhCheck :size="11" /> used
              </button>
            </div>
          </div>

          <div class="detail-fields">
            <div class="field">
              <span class="lbl">type</span>
              <template v-if="editField === 'type'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('type', selected.type)">
                {{ selected.type || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">category</span>
              <template v-if="editField === 'category'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('category', selected.category)">
                {{ selected.category || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">weight</span>
              <template v-if="editField === 'weight_oz'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('weight_oz', selected.weight_oz)">
                {{ selected.weight_oz ? selected.weight_oz + ' oz' : '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">qty</span>
              <template v-if="editField === 'qty'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('qty', selected.qty)">
                {{ selected.qty || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">container</span>
              <template v-if="editField === 'parent_container'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('parent_container', selected.parent_container)">
                {{ selected.parent_container || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">condition</span>
              <template v-if="editField === 'condition'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('condition', selected.condition)">
                {{ selected.condition || '—' }}
              </button>
            </div>

            <div class="field span2">
              <span class="lbl">location</span>
              <template v-if="editingLocation">
                <div class="loc-edit">
                  <input
                    ref="editInput"
                    v-model="editRoom"
                    placeholder="room"
                    @keydown.enter="saveLocation"
                    @keydown.escape="editingLocation = false"
                  />
                  <input
                    v-model="editDetail"
                    placeholder="detail"
                    @keydown.enter="saveLocation"
                    @keydown.escape="editingLocation = false"
                  />
                  <button class="mini-btn" @click="saveLocation"><PhCheck :size="11" /></button>
                  <button class="mini-btn" @click="editingLocation = false"><PhX :size="11" /></button>
                </div>
              </template>
              <button v-else class="val" @click="beginLocationEdit">
                <PhMapPin :size="10" />
                {{ selected.location_room || '—' }}
                <span v-if="selected.location_detail" class="muted">/ {{ selected.location_detail }}</span>
              </button>
            </div>

            <div class="field">
              <span class="lbl">purchase</span>
              <template v-if="editField === 'purchase_date'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  placeholder="YYYY-MM-DD"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('purchase_date', selected.purchase_date)">
                {{ selected.purchase_date || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">price</span>
              <template v-if="editField === 'purchase_price'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('purchase_price', selected.purchase_price)">
                {{ selected.purchase_price ? '$' + selected.purchase_price : '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">brand</span>
              <template v-if="editField === 'brand'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('brand', selected.brand)">
                {{ selected.brand || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">model</span>
              <template v-if="editField === 'model_number'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('model_number', selected.model_number)">
                {{ selected.model_number || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">serial</span>
              <template v-if="editField === 'serial_number'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val" @click="beginEdit('serial_number', selected.serial_number)">
                {{ selected.serial_number || '—' }}
              </button>
            </div>
            <div class="field">
              <span class="lbl">last used</span>
              <span class="val readonly" :class="{ stale: isStale(selected.last_used) }">
                <PhClock :size="10" />
                {{ selected.last_used || 'never' }}
              </span>
            </div>
          </div>

          <div class="detail-tags">
            <span class="lbl">tags</span>
            <template v-if="editField === 'tags'">
              <input
                ref="editInput"
                v-model="editValue"
                placeholder="comma-separated"
                @keydown.enter="saveField"
                @keydown.escape="editField = null"
                @blur="saveField"
              />
            </template>
            <template v-else>
              <button
                v-for="t in tagsList"
                :key="t"
                class="tag-chip"
                @click="beginEdit('tags', selected.tags)"
              >
                {{ t }}
              </button>
              <button v-if="!tagsList.length" class="tag-add" @click="beginEdit('tags', '')">
                <PhPencilSimple :size="10" /> add tags
              </button>
            </template>
          </div>

          <div class="detail-notes">
            <span class="lbl">notes</span>
            <template v-if="editField === 'notes'">
              <textarea
                ref="editInput"
                v-model="editValue"
                rows="3"
                @keydown.escape="editField = null"
                @blur="saveField"
              />
            </template>
            <button v-else class="notes-display" @click="beginEdit('notes', selected.notes)">
              <span v-if="selected.notes" class="notes-text">{{ selected.notes }}</span>
              <span v-else class="muted">
                <PhPencilSimple :size="10" /> add notes
              </span>
            </button>
          </div>

          <div class="detail-urls">
            <div class="field span2">
              <span class="lbl">photo url</span>
              <template v-if="editField === 'photo_url'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  placeholder="https://…"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val truncate" @click="beginEdit('photo_url', selected.photo_url)">
                {{ selected.photo_url || '—' }}
              </button>
            </div>
            <div class="field span2">
              <span class="lbl">scan url</span>
              <template v-if="editField === 'scan_3d_url'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  placeholder="https://…"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val truncate" @click="beginEdit('scan_3d_url', selected.scan_3d_url)">
                {{ selected.scan_3d_url || '—' }}
              </button>
            </div>
            <div class="field span2">
              <span class="lbl">amazon url</span>
              <template v-if="editField === 'amazon_url'">
                <input
                  ref="editInput"
                  v-model="editValue"
                  placeholder="https://…"
                  @keydown.enter="saveField"
                  @keydown.escape="editField = null"
                  @blur="saveField"
                />
              </template>
              <button v-else class="val truncate" @click="beginEdit('amazon_url', selected.amazon_url)">
                {{ selected.amazon_url || '—' }}
              </button>
            </div>
          </div>

          <div class="shortcuts">
            <span><kbd>u</kbd> used</span>
            <span><kbd>m</kbd> move</span>
            <span><kbd>R</kbd> new one</span>
            <span><kbd>l</kbd> location</span>
            <span><kbd>n</kbd> notes</span>
            <span><kbd>s</kbd> scan</span>
            <span><kbd>*</kbd> star</span>
            <span><kbd>e</kbd> tui</span>
            <span><kbd>c</kbd> commit</span>
            <span><kbd>↑↓</kbd> / <kbd>j/k</kbd> nav</span>
          </div>
        </div>
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

  /* Shared column template — both .sort-row (header) and .gear-row (data)
     consume this, so the columns line up vertically. Tight fixed widths on
     numeric / short cols keep enough flex room for name + container at
     narrow panel widths (the gear panel can get down to ~600px). */
  --gear-cols: minmax(0, 2.2fr) 48px 64px minmax(0, 1.5fr) 80px 70px;
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
  color: var(--accent);
  font-size: 11px;
}

.pending-badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  color: var(--warning);
  border: 1px solid var(--border-light);
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
  color: var(--accent);
  font-size: 11px;
}

.gear-controls {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 0 10px 0;
}
.gear-filter-row {
  display: flex;
  justify-content: flex-end;
}
/* Sort row is also the column header. Grid template comes from --gear-cols
   on .gear-panel, so headers line up over the data rows below. */
.sort-row {
  display: grid;
  grid-template-columns: var(--gear-cols);
  column-gap: 6px;
  align-items: center;
  border-bottom: 1px solid var(--border, #222);
}
.sort-row button {
  background: transparent;
  border: none;
  color: var(--text-tertiary, #777);
  font-size: 10px;
  padding: 4px 2px;
  cursor: pointer;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  font-variant-numeric: tabular-nums;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  /* Show that every header is interactive even before hover. */
  transition: color 0.12s ease, background 0.12s ease;
  min-width: 0;
  /* Truncate header label rather than overflow the next column. */
  overflow: hidden;
}
.sort-row button .hdr-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sort-row button:hover {
  color: var(--text-secondary, #aaa);
  background: rgba(255, 255, 255, 0.03);
}
.sort-row button.active {
  color: var(--text-primary, #fff);
}
/* Right-align the numeric weight header to match its data column. */
.sort-row button.col-weight {
  justify-content: flex-end;
}
/* Reserve a fixed slot for the direction chevron on every header — keeps
   the label position stable when sort changes between columns. */
.sort-row .dir {
  display: inline-block;
  width: 9px;
  font-size: 9px;
  text-align: center;
  color: var(--accent);
}
.container-select {
  background: var(--bg-alt, #0d0d0d);
  color: inherit;
  border: 1px solid var(--border, #222);
  border-radius: 3px;
  padding: 3px 6px;
  font-size: 10px;
  font-family: inherit;
  max-width: 180px;
}

.gear-list {
  flex: 1;
  overflow-y: auto;
  border-top: 1px solid var(--border, #222);
}

.gear-row {
  display: grid;
  grid-template-columns: var(--gear-cols);
  column-gap: 6px;
  align-items: center;
  /* Tight rows — content is short labels and small chips, no need for the
     old 4px top/bottom that made each row ~24px tall. */
  padding: 1px 10px;
  cursor: pointer;
  border-bottom: 1px solid #161616;
  line-height: 1.3;
}

.gear-row.active {
  background: var(--accent-soft);
  color: #fff;
}

/* Every cell shares the same overflow / muted-meta defaults. The name column
   overrides to be the brighter, primary text. */
.gear-row > span {
  min-width: 0;
  font-size: 10px;
  color: var(--muted, #888);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.gear-row .col-name {
  font-size: 12px;
  color: inherit;
  gap: 5px;
}
.gear-row .col-weight {
  justify-content: flex-end;
  font-variant-numeric: tabular-nums;
}
.gear-row .col-used {
  font-variant-numeric: tabular-nums;
}
.gear-row .col-used.stale {
  color: var(--warning);
}
.gear-row.active > span {
  color: var(--accent-soft);
}
.gear-row.active .col-name {
  color: #fff;
}

.row-type-icon {
  color: var(--muted, #777);
  flex-shrink: 0;
}
.gear-row.active .row-type-icon {
  color: var(--accent-soft);
}
.row-star {
  color: var(--warning);
  flex-shrink: 0;
}

.gear-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.type-chip {
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid var(--border, #222);
  border-radius: 2px;
  padding: 0 5px;
  font-size: 9px;
  text-transform: lowercase;
  color: var(--text-tertiary, #888);
}
.gear-row.active .type-chip {
  background: rgba(0, 0, 0, 0.18);
  border-color: rgba(255, 255, 255, 0.18);
  color: #fff;
}

.container-chip {
  background: transparent;
  border: none;
  padding: 0 4px;
  margin: 0;
  border-radius: 2px;
  font-size: 9px;
  font-family: inherit;
  color: var(--text-tertiary, #777);
  text-transform: lowercase;
  cursor: pointer;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: background 0.1s ease, color 0.1s ease;
}
.container-chip:hover {
  background: var(--accent-softer);
  color: var(--text-secondary, #aaa);
}
.container-chip.active {
  background: var(--accent-soft);
  color: var(--text-primary, #fff);
}
.gear-row.active .container-chip {
  color: var(--accent-soft);
}
.gear-row.active .container-chip.active {
  background: rgba(0, 0, 0, 0.2);
  color: #fff;
}
.container-fallback {
  font-size: 9px;
  color: var(--text-tertiary, #555);
  font-style: italic;
  opacity: 0.7;
}

.weight {
  font-variant-numeric: tabular-nums;
  font-size: 9px;
  color: var(--text-tertiary, #888);
}
.gear-row.active .weight {
  color: var(--accent-soft);
}

/* Old .gear-meta .loc / .last styling rolled into the .col-* cells above
   when rows moved to a CSS grid. */

.scan-icon {
  color: var(--accent);
  opacity: 0.7;
}

.empty-row {
  padding: 14px 10px;
  color: var(--muted, #666);
  text-align: center;
}

/* Resize divider now provided by <ResizeHandle axis="y"> — see imports. */

.gear-detail {
  border-top: 1px solid var(--border, #222);
  overflow-y: auto;
  flex-shrink: 0;
  background: var(--bg-alt, #0a0a0a);
}

.detail-card {
  display: grid;
  grid-template-columns: 140px 1fr;
  gap: 14px;
  padding: 12px;
}

.detail-photo {
  width: 140px;
  height: 140px;
  border-radius: 4px;
  overflow: hidden;
  background: #0d0d0d;
  border: 1px solid var(--border, #222);
  display: flex;
  align-items: center;
  justify-content: center;
}
.detail-photo img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.detail-photo.empty {
  border-style: dashed;
}
.photo-add {
  background: transparent;
  border: 0;
  color: var(--muted, #555);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: center;
  font-size: 10px;
}
.photo-add:hover {
  color: var(--text-secondary, #aaa);
}

.detail-body {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.detail-headline {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.star-btn {
  background: transparent;
  border: 0;
  color: var(--muted, #555);
  cursor: pointer;
  padding: 2px;
  margin-top: 1px;
}
.star-btn:hover {
  color: var(--warning);
}
.star-btn .starred {
  color: var(--warning);
}

.detail-name-block {
  flex: 1;
  min-width: 0;
}

.detail-name {
  font-weight: 600;
  font-size: 13px;
  line-height: 1.2;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.detail-sub {
  font-size: 10px;
  color: var(--text-tertiary, #888);
  margin-top: 2px;
}

.headline-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

.link-btn {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  background: transparent;
  color: var(--text-secondary, #aaa);
  border: 1px solid var(--border, #222);
  border-radius: 3px;
  padding: 2px 7px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
}
.link-btn:hover {
  background: var(--bg-alt, #161616);
  color: var(--text, #ddd);
}
.link-btn.primary {
  border-color: var(--accent-soft);
  color: var(--accent);
}
.link-btn.primary:hover {
  background: var(--accent-soft);
  color: #fff;
}

.detail-fields {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 4px 12px;
}
.detail-fields .field.span2 {
  grid-column: span 3;
}

.field {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 11px;
}

.lbl {
  color: var(--muted, #666);
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  flex-shrink: 0;
  min-width: 52px;
}

.val {
  background: transparent;
  border: 1px solid transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  padding: 2px 5px;
  border-radius: 2px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.val:hover {
  background: rgba(255, 255, 255, 0.04);
  border-color: var(--border, #222);
}
.val.truncate {
  font-variant-numeric: tabular-nums;
  font-size: 10px;
  color: var(--text-tertiary, #888);
}
.val.readonly {
  cursor: default;
}
.val.readonly:hover {
  background: transparent;
  border-color: transparent;
}
.val.readonly.stale {
  color: var(--warning);
}

.field input,
.field textarea {
  flex: 1;
  background: var(--bg, #050505);
  color: inherit;
  border: 1px solid var(--accent-soft);
  border-radius: 2px;
  padding: 2px 5px;
  font-size: 11px;
  font-family: inherit;
  min-width: 0;
}

.loc-edit {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
}
.loc-edit input {
  flex: 1;
}

.mini-btn {
  background: transparent;
  color: var(--text-secondary, #aaa);
  border: 1px solid var(--border, #222);
  border-radius: 2px;
  padding: 2px 4px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
}
.mini-btn:hover {
  background: var(--bg-alt, #161616);
}

.detail-tags {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.detail-tags input {
  flex: 1;
  background: var(--bg, #050505);
  color: inherit;
  border: 1px solid var(--accent-soft);
  border-radius: 2px;
  padding: 2px 5px;
  font-size: 11px;
  font-family: inherit;
}
.tag-chip {
  background: rgba(110, 237, 247, 0.08);
  border: 1px solid rgba(110, 237, 247, 0.2);
  color: var(--accent);
  border-radius: 10px;
  padding: 1px 8px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
}
.tag-chip:hover {
  background: rgba(110, 237, 247, 0.18);
}
.tag-add {
  background: transparent;
  border: 1px dashed var(--border, #222);
  color: var(--muted, #666);
  border-radius: 10px;
  padding: 1px 8px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.tag-add:hover {
  color: var(--text-secondary, #aaa);
}

.detail-notes {
  display: flex;
  align-items: flex-start;
  gap: 6px;
}
.detail-notes .lbl {
  margin-top: 4px;
}
.detail-notes textarea {
  flex: 1;
  resize: vertical;
  min-height: 56px;
  line-height: 1.4;
}
.notes-display {
  flex: 1;
  background: transparent;
  border: 1px solid transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  padding: 4px 6px;
  border-radius: 3px;
  cursor: pointer;
  min-height: 28px;
  line-height: 1.4;
  display: flex;
  align-items: flex-start;
  gap: 4px;
}
.notes-display:hover {
  background: rgba(255, 255, 255, 0.03);
  border-color: var(--border, #222);
}
.notes-text {
  white-space: pre-wrap;
  word-break: break-word;
}

.detail-urls {
  display: grid;
  grid-template-columns: 1fr;
  gap: 2px;
}

.shortcuts {
  margin-top: 4px;
  font-size: 9px;
  color: var(--muted, #555);
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
  font-size: 9px;
}
</style>
