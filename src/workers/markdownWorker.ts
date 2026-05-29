// Markdown rendering worker — runs the entire unified pipeline off the
// main thread so file switching never blocks Dispatch's UI. The main
// thread sends `{ id, content, baseUrl }`, the worker responds with
// `{ id, html }` or `{ id, error }`.
//
// We process every message we receive (no internal queueing) but tag each
// response with the request id, so if the user clicks multiple files
// rapidly the main thread can throw away stale responses by id check.

import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeRaw from 'rehype-raw'
import rehypeStringify from 'rehype-stringify'
import { remarkObsidianWikilinks } from '../utils/remarkObsidianWikilinks'
import { remarkMermaid } from '../utils/remarkMermaid'

interface RenderRequest {
  id: number
  content: string
  baseUrl: string
}

interface RenderResponse {
  id: number
  html?: string
  error?: string
}

// Cache the processor by baseUrl — building a unified pipeline is
// nontrivial work (registering each plugin's transformers) and the
// baseUrl only changes when the user switches publish target.
// Typed as `any` because the generic chain through .use(...) is
// hellish to express and we don't gain anything from the strict types
// inside the worker — the public surface is just (string) => string.
const processorCache = new Map<string, any>()

function getProcessor(baseUrl: string): any {
  const cached = processorCache.get(baseUrl)
  if (cached) return cached
  const built = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkObsidianWikilinks, { baseUrl })
    .use(remarkMermaid)
    .use(remarkRehype, { allowDangerousHtml: true })
    .use(rehypeRaw)
    .use(rehypeStringify, { allowDangerousHtml: true })
  processorCache.set(baseUrl, built)
  return built
}

self.addEventListener('message', async (e: MessageEvent<RenderRequest>) => {
  const { id, content, baseUrl } = e.data
  try {
    const result = await getProcessor(baseUrl).process(content)
    const html = String(result)
    const res: RenderResponse = { id, html }
    ;(self as unknown as Worker).postMessage(res)
  } catch (err) {
    const res: RenderResponse = {
      id,
      error: err instanceof Error ? err.message : String(err),
    }
    ;(self as unknown as Worker).postMessage(res)
  }
})

export {}
