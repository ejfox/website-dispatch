<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { PhMinus, PhPlus, PhX } from '@phosphor-icons/vue'
import UiButton from '../ui/UiButton.vue'
import UiButtonGroup from '../ui/UiButtonGroup.vue'
import SettingsRow from './SettingsRow.vue'
import { useSettings } from '../../composables/useSettings'
import type { RepoCheck, VaultCheck } from '../../types'

const { config, commit } = useSettings()

// ── Which site is being edited (most people have one) ───────────────────────
const targets = computed(() => config.value?.publish_targets ?? [])
const index = ref(Math.max(0, targets.value.findIndex((t) => t.is_default)))
const target = computed(() => targets.value[index.value] ?? null)

function addTarget() {
  if (!config.value) return
  config.value.publish_targets.push({
    name: 'New site',
    id: `target-${Date.now()}`,
    repo_path: '',
    domain: 'https://',
    content_path_pattern: 'content/blog/{year}',
    branch: 'main',
    is_default: config.value.publish_targets.length === 0,
  })
  index.value = config.value.publish_targets.length - 1
  commit()
}

function removeTarget() {
  if (!config.value || targets.value.length < 2) return
  const [removed] = config.value.publish_targets.splice(index.value, 1)
  if (removed?.is_default) config.value.publish_targets[0].is_default = true
  index.value = 0
  commit()
}

function makeDefault() {
  if (!config.value) return
  config.value.publish_targets.forEach((t, i) => (t.is_default = i === index.value))
  commit()
}

// ── Repo checks: exists, git, on the configured branch, has origin ──────────
const repo = ref<RepoCheck | null>(null)
const repoPathDraft = ref('')
const repoError = ref<string | null>(null)

async function checkRepo() {
  const t = target.value
  if (!t) return
  try {
    repo.value = (await invoke('check_publish_target', {
      repoPath: t.repo_path,
      branch: t.branch,
      contentPathPattern: t.content_path_pattern,
    })) as RepoCheck
  } catch {
    repo.value = null
  }
}

watch(
  target,
  (t) => {
    repoPathDraft.value = t?.repo_path ?? ''
    repoError.value = null
    checkRepo()
  },
  { immediate: true },
)

async function commitRepoPath(path: string) {
  const t = target.value
  if (!t) return
  const trimmed = path.trim()
  const check = (await invoke('check_publish_target', {
    repoPath: trimmed,
    branch: t.branch,
    contentPathPattern: t.content_path_pattern,
  }).catch(() => null)) as RepoCheck | null
  if (!check?.exists) {
    repoError.value = 'That folder doesn’t exist. Dispatch is still using the previous one.'
    return
  }
  repoError.value = null
  repo.value = check
  if (trimmed !== t.repo_path) {
    t.repo_path = trimmed
    commit()
  }
}

async function browseRepo() {
  const selected = await open({
    directory: true,
    title: 'Choose your website’s git repository',
    defaultPath: repoPathDraft.value || undefined,
  })
  if (typeof selected === 'string') {
    repoPathDraft.value = selected
    commitRepoPath(selected)
  }
}

function commitAndRecheck() {
  commit()
  checkRepo()
}

const repoNote = computed<{ tone: string; text: string } | null>(() => {
  const r = repo.value
  const t = target.value
  if (!r || !t || repoError.value) return null
  if (!r.exists) return { tone: 'is-error', text: 'This folder doesn’t exist.' }
  if (!r.is_git) return { tone: 'is-error', text: 'This folder isn’t a git repository, so there’s nothing to publish to.' }
  if (!r.has_origin) return { tone: 'is-warn', text: 'Git repository found, but it has no “origin” remote to push to.' }
  return { tone: 'is-ok', text: 'Git repository with an origin remote.' }
})

const branchNote = computed<{ tone: string; text: string } | null>(() => {
  const r = repo.value
  const t = target.value
  if (!r?.is_git || !t) return null
  if (!t.branch.trim()) return { tone: 'is-error', text: 'Enter the branch your site deploys from.' }
  if (r.on_branch) return { tone: 'is-ok', text: `The repo is on ${t.branch}.` }
  if (!r.branch_exists) return { tone: 'is-error', text: `There’s no branch named ${t.branch} in this repo.` }
  return {
    tone: 'is-warn',
    text: `The repo is on ${r.current_branch || 'a detached HEAD'}, not ${t.branch}. Publishing waits until you switch back.`,
  }
})

// The address is edited as a draft and saved only once it's a real URL: a
// bad one would break every post link and the deploy check.
const domainDraft = ref('')
watch(target, (t) => (domainDraft.value = t?.domain ?? ''), { immediate: true })
function commitDomain() {
  const t = target.value
  if (!t || domainNote.value) return
  const d = domainDraft.value.trim().replace(/\/+$/, '')
  domainDraft.value = d
  if (d !== t.domain) {
    t.domain = d
    commit()
  }
}

const domainNote = computed<string | null>(() => {
  const d = domainDraft.value.trim()
  if (!d) return 'Enter the address your site is served at.'
  try {
    const u = new URL(d)
    if (u.protocol !== 'https:' && u.protocol !== 'http:') return 'Start the address with https://'
    if (!u.hostname.includes('.')) return 'That doesn’t look like a full web address.'
    return null
  } catch {
    return 'Start the address with https://'
  }
})

// ── Publishable folders (vault side) ────────────────────────────────────────
// Same rule as publish_guard.rs: week-notes/ and drafts/ are never publishable.
const NEVER = /(^|\/)(week-notes|drafts)(\/|$)/i
const newDir = ref('')
const dirError = ref<string | null>(null)
const missingDirs = ref<string[]>([])

async function checkDirs() {
  if (!config.value) return
  try {
    const c = (await invoke('check_vault_folder', {
      path: config.value.vault.path,
      publishableDirs: config.value.vault.publishable_dirs,
    })) as VaultCheck
    missingDirs.value = c.exists ? c.missing_dirs : []
  } catch {
    missingDirs.value = []
  }
}
checkDirs()

function addDir() {
  if (!config.value) return
  const dir = newDir.value.trim().replace(/^\/+|\/+$/g, '')
  if (!dir) return
  if (NEVER.test(dir)) {
    dirError.value = `${dir} can never be published: drafts and week notes stay private.`
    return
  }
  dirError.value = null
  if (!config.value.vault.publishable_dirs.includes(dir)) {
    config.value.vault.publishable_dirs.push(dir)
    commit()
    checkDirs()
  }
  newDir.value = ''
}

function removeDir(i: number) {
  if (!config.value) return
  config.value.vault.publishable_dirs.splice(i, 1)
  commit()
}
</script>

<template>
  <div v-if="config" class="st-form">
    <template v-if="targets.length > 1">
      <SettingsRow label="Site:" for="st-target" help="Each site is its own repo. Publish uses the default unless you pick another.">
        <div class="st-line">
          <select id="st-target" v-model="index" class="st-select">
            <option v-for="(t, i) in targets" :key="t.id" :value="i">
              {{ t.name }}{{ t.is_default ? ' (default)' : '' }}
            </option>
          </select>
          <UiButtonGroup label="Add or remove sites">
            <UiButton size="small" :icon="PhPlus" aria-label="Add a site" @click="addTarget" />
            <UiButton size="small" :icon="PhMinus" aria-label="Remove this site" @click="removeTarget" />
          </UiButtonGroup>
        </div>
      </SettingsRow>
      <SettingsRow v-if="target" label="Name:" for="st-target-name">
        <div class="st-line">
          <input id="st-target-name" v-model="target.name" class="st-input is-short" @change="commit()" />
          <label class="st-check">
            <input type="checkbox" :checked="target.is_default" :disabled="target.is_default" @change="makeDefault" />
            Default
          </label>
        </div>
      </SettingsRow>
    </template>

    <template v-if="target">
      <SettingsRow
        label="Website repository:"
        for="st-repo"
        help="The git checkout of your site. Publishing copies the post here, commits it and pushes."
      >
        <div class="st-line">
          <input
            id="st-repo"
            v-model="repoPathDraft"
            class="st-input is-mono"
            spellcheck="false"
            :aria-invalid="!!repoError || repo?.exists === false || (repo?.exists && !repo.is_git)"
            @change="commitRepoPath(repoPathDraft)"
          />
          <UiButton size="small" label="Choose…" @click="browseRepo" />
        </div>
        <template #note>
          <p v-if="repoError" class="st-note is-error">{{ repoError }}</p>
          <p v-else-if="repoNote" class="st-note" :class="repoNote.tone">{{ repoNote.text }}</p>
        </template>
      </SettingsRow>

      <SettingsRow
        label="Branch:"
        for="st-branch"
        help="The branch your host deploys from. Dispatch won’t publish while the repo is on any other branch."
      >
        <input
          id="st-branch"
          v-model="target.branch"
          class="st-input is-short is-mono"
          spellcheck="false"
          :aria-invalid="branchNote?.tone === 'is-error'"
          @change="commitAndRecheck"
        />
        <template #note>
          <p v-if="branchNote" class="st-note" :class="branchNote.tone">{{ branchNote.text }}</p>
        </template>
      </SettingsRow>

      <SettingsRow
        label="Site address:"
        for="st-domain"
        help="Where the site is live. Used for post links, “View on site” and checking that a deploy finished."
      >
        <input
          id="st-domain"
          v-model="domainDraft"
          class="st-input"
          placeholder="https://example.com"
          spellcheck="false"
          :aria-invalid="!!domainNote"
          @change="commitDomain"
        />
        <template #note>
          <p v-if="domainNote" class="st-note is-error">{{ domainNote }} Still using {{ target.domain || 'nothing' }}.</p>
        </template>
      </SettingsRow>

      <SettingsRow
        label="Posts go in:"
        for="st-pattern"
        help="The folder inside the repo where a published post is written. {year} becomes the post’s year."
      >
        <input
          id="st-pattern"
          v-model="target.content_path_pattern"
          class="st-input is-mono"
          placeholder="content/blog/{year}"
          spellcheck="false"
          @change="commitAndRecheck"
        />
        <template #note>
          <p v-if="repo?.exists && repo.content_dir && !repo.content_dir_exists" class="st-note is-warn">
            There’s no <code>{{ repo.content_dir }}</code> folder in the repo yet.
          </p>
        </template>
      </SettingsRow>
    </template>

    <div class="st-section"><h3>From your vault</h3></div>

    <SettingsRow
      label="Publishable folders:"
      help="Only notes inside these vault folders appear in Dispatch and can be published. Drafts and week notes never can."
    >
      <div class="st-chips">
        <span
          v-for="(d, i) in config.vault.publishable_dirs"
          :key="d"
          class="st-chip"
          :class="{ 'is-missing': missingDirs.includes(d) }"
          :title="missingDirs.includes(d) ? 'This folder isn’t in your vault' : undefined"
        >
          {{ d }}
          <UiButton variant="plain" size="small" :icon="PhX" :aria-label="`Stop publishing from ${d}`" @click="removeDir(i)" />
        </span>
        <input
          v-model="newDir"
          class="st-input is-short is-mono"
          placeholder="Add a folder"
          spellcheck="false"
          :aria-invalid="!!dirError"
          @keydown.enter="addDir"
          @change="addDir"
        />
      </div>
      <template #note>
        <p v-if="dirError" class="st-note is-error">{{ dirError }}</p>
        <p v-else-if="missingDirs.length" class="st-note is-warn">
          Not in your vault: {{ missingDirs.join(', ') }}.
        </p>
        <p v-if="!config.vault.publishable_dirs.length" class="st-note is-warn">
          With no publishable folders, Dispatch shows no posts.
        </p>
      </template>
    </SettingsRow>

    <SettingsRow v-if="targets.length < 2" label="">
      <UiButton size="small" :icon="PhPlus" label="Add another site…" @click="addTarget" />
    </SettingsRow>
  </div>
</template>
