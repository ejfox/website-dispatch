/**
 * Lazy mermaid renderer for the preview pane.
 * Pairs with remarkMermaid (emits <pre class="mermaid">SOURCE</pre>).
 */
let mermaidPromise: Promise<any> | null = null

function loadMermaid() {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((m) => {
      const mermaid = m.default
      mermaid.initialize({
        startOnLoad: false,
        theme: 'dark',
        securityLevel: 'strict',
      })
      return mermaid
    })
  }
  return mermaidPromise
}

export async function renderMermaidIn(root: HTMLElement | Document = document) {
  const blocks = root.querySelectorAll<HTMLElement>(
    'pre.mermaid:not([data-mermaid-processed])',
  )
  if (!blocks.length) return

  const mermaid = await loadMermaid()

  for (const block of Array.from(blocks)) {
    block.setAttribute('data-mermaid-processed', 'true')
    const source = block.textContent || ''
    const id = `mermaid-${Math.random().toString(36).slice(2, 10)}`
    try {
      const { svg } = await mermaid.render(id, source)
      const wrapper = document.createElement('div')
      wrapper.className = 'mermaid-diagram'
      wrapper.innerHTML = svg
      block.replaceWith(wrapper)
    } catch (err) {
      console.warn('[mermaid] render failed', err)
      block.setAttribute('data-mermaid-error', 'true')
    }
  }
}
