/**
 * Converts ```mermaid fenced code blocks into raw HTML <pre class="mermaid">
 * nodes so subsequent passes leave them alone. mermaid.js renders them
 * client-side after the HTML is mounted.
 */
import { visit } from 'unist-util-visit'
import type { Plugin } from 'unified'
import type { Root } from 'mdast'

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

export const remarkMermaid: Plugin<[], Root> = () => (tree) => {
  visit(tree, 'code', (node: any) => {
    if (node.lang !== 'mermaid') return
    node.type = 'html'
    node.value = `<pre class="mermaid">${escapeHtml(node.value)}</pre>`
    delete node.lang
    delete node.meta
  })
}
