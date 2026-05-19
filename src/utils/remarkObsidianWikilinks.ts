/**
 * Converts Obsidian [[wikilinks]] (with optional |alias and #heading) into
 * standard markdown links pointing to the same URLs website2 would generate.
 *
 * Bare wikilinks (e.g. `[[OSINT]]`, `[[Internet Culture]]`) are treated as
 * topic/tag references and routed to `/tag/{slug}`. Pathed wikilinks
 * (e.g. `[[blog/2026/foo]]`) keep their canonical route.
 *
 * Pass `baseUrl` to emit absolute URLs (so they open in a real browser when
 * clicked from the Tauri webview). Links get target="_blank" rel="noopener".
 */
import { visit } from 'unist-util-visit'
import type { Plugin } from 'unified'
import type { Root } from 'mdast'

export interface ObsidianWikilinksOptions {
  baseUrl?: string
}

const generateSlug = (s: string) =>
  s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')

const encodePath = (p: string) =>
  p.split('/').filter(Boolean).map(encodeURIComponent).join('/')

function normalizeTarget(raw: string) {
  let t = raw.trim().replace(/\\/g, '/')
  if (t.startsWith('/')) t = t.slice(1)
  if (t.endsWith('.md')) t = t.slice(0, -3)
  while (t.startsWith('../')) t = t.slice(3)
  if (t.startsWith('./')) t = t.slice(2)
  return t.replace(/\/{2,}/g, '/')
}

function buildInternalHref(target: string) {
  const n = normalizeTarget(target)
  const lower = n.toLowerCase()
  if (lower.startsWith('reading/')) return `/reading/${encodePath(n.slice(8))}`
  if (lower.startsWith('projects/')) return `/projects/${encodePath(n.slice(9))}`
  if (lower.startsWith('robots/')) return `/blog/robots/${encodePath(n.slice(7))}`
  if (lower.startsWith('week-notes/'))
    return `/blog/week-notes/${encodePath(n.slice(11))}`
  if (lower.startsWith('blog/')) return `/blog/${encodePath(n.slice(5))}`
  // Bare wikilinks → tag page (no slash = topic reference, not a file path)
  if (!n.includes('/')) return `/tag/${generateSlug(n)}`
  return `/blog/${encodePath(n)}`
}

export const remarkObsidianWikilinks: Plugin<[ObsidianWikilinksOptions?], Root> =
  (opts = {}) => {
    const base = (opts.baseUrl || '').replace(/\/$/, '')

    return (tree) => {
      visit(tree, 'text', (node: any, index, parent: any) => {
        if (!parent || index == null) return
        const value: string = node.value
        if (!value.includes('[[')) return

        const regex = /\[\[([^\]]+)\]\]/g
        const nodes: any[] = []
        let last = 0
        let m: RegExpExecArray | null

        while ((m = regex.exec(value)) !== null) {
          if (m.index > last) {
            nodes.push({ type: 'text', value: value.slice(last, m.index) })
          }
          const linkText = m[1]
          const [pathAndHeading, aliasPart] = linkText.split('|')
          const [rawTarget, rawHeading] = pathAndHeading.split('#')
          const target = normalizeTarget(rawTarget)
          const alias = aliasPart?.trim() || target

          let url = buildInternalHref(target)
          if (rawHeading) url += `#${generateSlug(rawHeading.trim())}`
          if (base) url = `${base}${url}`

          nodes.push({
            type: 'link',
            url,
            children: [{ type: 'text', value: alias }],
            data: {
              hProperties: {
                className: 'internal-link',
                target: '_blank',
                rel: 'noopener noreferrer',
              },
            },
          })
          last = regex.lastIndex
        }

        if (last === 0) return
        if (last < value.length) {
          nodes.push({ type: 'text', value: value.slice(last) })
        }

        parent.children.splice(index, 1, ...nodes)
        return [visit.SKIP, index + nodes.length] as any
      })
    }
  }
