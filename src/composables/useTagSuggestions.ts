import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useAppConfig } from './useAppConfig'

export function useTagSuggestions(options: {
  getFilePath: () => string
  getFileTags: () => string[]
  onFeedback: (msg: string) => void
  onRefresh: () => void
}) {
  const { appConfig } = useAppConfig()
  const availableTags = ref<Record<string, number>>({})
  const suggestedTags = ref<string[]>([])
  const loadingTags = ref(false)
  const addingTag = ref(false)

  async function fetchAvailableTags() {
    if (Object.keys(availableTags.value).length > 0) return
    loadingTags.value = true
    try {
      const defaultTarget = (appConfig.value as any)?.publish_targets?.find((t: any) => t.is_default)
      const domain = defaultTarget?.domain || 'https://ejfox.com'
      const res = await fetch(`${domain}/content-tags.json`)
      availableTags.value = await res.json()
    } catch (e) {
      console.log('Could not fetch tags:', e)
    }
    loadingTags.value = false
  }

  const tagKeywords: Record<string, string[]> = {
    politics: [
      'trump',
      'biden',
      'election',
      'congress',
      'democrat',
      'republican',
      'vote',
      'political',
      'government',
      'ice',
      'immigration',
    ],
    coding: ['code', 'programming', 'javascript', 'python', 'typescript', 'function', 'api', 'software', 'developer'],
    photography: ['photo', 'camera', 'lens', 'shot', 'film', 'photograph'],
    art: ['painting', 'drawing', 'canvas', 'artist', 'creative', 'sketch'],
    music: ['song', 'album', 'band', 'guitar', 'piano', 'spotify', 'playlist'],
    design: ['design', 'ui', 'ux', 'interface', 'layout', 'figma'],
    writing: ['write', 'essay', 'blog', 'draft', 'author', 'story'],
    personal: ['i feel', 'my life', 'personal', 'journal', 'diary', 'reflection'],
    activism: ['protest', 'march', 'activist', 'movement', 'justice', 'rights'],
    travel: ['travel', 'trip', 'flight', 'hotel', 'vacation', 'city', 'country'],
    video: ['video', 'film', 'documentary', 'youtube', 'cinema'],
    javascript: ['javascript', 'js', 'node', 'npm', 'react', 'vue'],
    dataviz: ['visualization', 'chart', 'graph', 'd3', 'data viz'],
    security: ['security', 'hack', 'encrypt', 'password', 'vulnerability'],
  }

  function analyzeTags(text: string, existingTags: string[]): string[] {
    const tags = Object.keys(availableTags.value)
    if (tags.length === 0) return []

    const textLower = text.toLowerCase()
    const suggestions: { tag: string; score: number }[] = []

    for (const tag of tags) {
      if (existingTags.map((t) => t.toLowerCase()).includes(tag.toLowerCase())) continue

      let score = 0
      if (textLower.includes(tag.toLowerCase())) score += 3

      const keywords = tagKeywords[tag] || []
      for (const keyword of keywords) {
        if (textLower.includes(keyword)) score += 2
      }

      const usageBoost = Math.min(availableTags.value[tag] / 50, 0.5)
      score += usageBoost

      if (score > 0) suggestions.push({ tag, score })
    }

    return suggestions
      .sort((a, b) => b.score - a.score)
      .slice(0, 5)
      .map((s) => s.tag)
  }

  async function addTag(tag: string) {
    if (addingTag.value) return
    addingTag.value = true
    try {
      await invoke('add_tag_to_file', { path: options.getFilePath(), tag })
      options.onFeedback(`Added: ${tag}`)
      suggestedTags.value = suggestedTags.value.filter((t) => t !== tag)
      options.onRefresh()
    } catch (e) {
      options.onFeedback(`Failed to add tag`)
      console.error('Failed to add tag:', e)
    }
    addingTag.value = false
  }

  // Auto-fetch on creation
  fetchAvailableTags()

  return { availableTags, suggestedTags, loadingTags, addingTag, fetchAvailableTags, analyzeTags, addTag }
}
