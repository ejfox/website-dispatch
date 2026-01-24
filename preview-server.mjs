#!/usr/bin/env node
/**
 * Preview server that uses the EXACT pipeline from website2
 * Uses the modular markdownToHtml.mjs for fast conversions
 */

import { createServer } from 'http'
import { promises as fs } from 'fs'
import path from 'path'
import { spawn } from 'child_process'

const WEBSITE2_PATH = '/Users/ejfox/code/website2'
const PORT = 6419
let currentFile = ''

// Spawn a child process in website2 using the lightweight converter
async function processWithWebsite2(filePath) {
  return new Promise((resolve, reject) => {
    // Use the preview-convert.mjs wrapper script
    const child = spawn('node', ['scripts/preview-convert.mjs', filePath], {
      cwd: WEBSITE2_PATH,
      env: { ...process.env, NODE_NO_WARNINGS: '1' }
    })

    let stdout = ''
    let stderr = ''

    child.stdout.on('data', data => stdout += data)
    child.stderr.on('data', data => stderr += data)

    child.on('close', code => {
      if (code === 0) {
        try {
          // Find the JSON in the output (skip any dotenv messages)
          const lines = stdout.split('\n')
          const jsonLine = lines.find(l => l.startsWith('{'))
          if (jsonLine) {
            resolve(JSON.parse(jsonLine))
          } else {
            reject(new Error('No JSON output'))
          }
        } catch (e) {
          reject(new Error(`Parse error: ${e.message}`))
        }
      } else {
        reject(new Error(stderr || `Exit code ${code}`))
      }
    })

    // Timeout after 30s
    setTimeout(() => {
      child.kill()
      reject(new Error('Processing timeout'))
    }, 30000)
  })
}

const HTML = `<!DOCTYPE html>
<html class="dark">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Dispatch Preview</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <script>
    tailwind.config = {
      darkMode: 'class',
      theme: {
        extend: {
          fontFamily: {
            serif: ['Georgia', 'Times New Roman', 'serif'],
          }
        }
      }
    }
  </script>
  <style>
    body {
      font-family: Georgia, 'Times New Roman', serif;
      background: #09090b;
      color: #e4e4e7;
    }
    .prose { max-width: 65ch; }
    .prose h1, .prose h2, .prose h3, .prose h4 {
      font-family: -apple-system, BlinkMacSystemFont, sans-serif;
      color: #fafafa;
      font-weight: 600;
    }
    .prose h1 { font-size: 2em; margin-top: 0; }
    .prose h2 { font-size: 1.5em; }
    .prose h3 { font-size: 1.25em; }
    .prose p { margin: 1em 0; line-height: 1.7; }
    .prose a { color: #3b82f6; text-decoration: none; }
    .prose a:hover { text-decoration: underline; }
    .prose code {
      background: #27272a;
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 0.85em;
      font-family: 'SF Mono', Menlo, monospace;
    }
    .prose pre {
      background: #18181b;
      border: 1px solid #27272a;
      border-radius: 8px;
      padding: 1rem;
      overflow-x: auto;
      margin: 1.5em 0;
    }
    .prose pre code { background: none; padding: 0; }
    .prose blockquote {
      border-left: 4px solid #3b82f6;
      padding-left: 1rem;
      margin: 1.5em 0;
      color: #a1a1aa;
      font-style: italic;
    }
    .prose img { border-radius: 8px; max-width: 100%; }
    .prose hr { border: none; border-top: 1px solid #27272a; margin: 2em 0; }
    .prose ul, .prose ol { padding-left: 1.5em; margin: 1em 0; }
    .prose li { margin: 0.5em 0; }
    .prose table { border-collapse: collapse; width: 100%; margin: 1.5em 0; }
    .prose th, .prose td { border: 1px solid #27272a; padding: 0.75rem 1rem; text-align: left; }
    .prose th { background: #18181b; font-weight: 600; }

    /* Code highlighting */
    [data-rehype-pretty-code-figure] { margin: 1.5em 0; }
    [data-rehype-pretty-code-figure] pre { margin: 0; }
    figure[data-rehype-pretty-code-figure] pre {
      background: #1e1e2e !important;
      border: 1px solid #313244;
    }

    /* Footnotes */
    .footnotes { font-size: 0.875em; color: #a1a1aa; margin-top: 3em; border-top: 1px solid #27272a; padding-top: 1em; }
    sup a { color: #3b82f6; }

    .status {
      position: fixed;
      top: 12px;
      right: 12px;
      padding: 6px 12px;
      background: #22c55e;
      color: #000;
      border-radius: 6px;
      font-size: 12px;
      font-weight: 600;
      font-family: -apple-system, sans-serif;
      opacity: 0;
      transition: opacity 0.3s;
    }
    .status.show { opacity: 1; }
    .empty {
      color: #52525b;
      text-align: center;
      padding: 100px 20px;
      font-family: -apple-system, sans-serif;
    }
    .filename {
      position: fixed;
      bottom: 12px;
      left: 12px;
      font-size: 11px;
      color: #3f3f46;
      font-family: 'SF Mono', monospace;
    }
    .processing {
      position: fixed;
      top: 12px;
      left: 12px;
      font-size: 11px;
      color: #a1a1aa;
      font-family: -apple-system, sans-serif;
    }
    /* TOC sidebar */
    .toc-sidebar {
      position: fixed;
      top: 60px;
      right: 20px;
      width: 200px;
      max-height: calc(100vh - 100px);
      overflow-y: auto;
      font-family: -apple-system, sans-serif;
      font-size: 11px;
    }
    .toc-title {
      color: #52525b;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-bottom: 8px;
    }
    .toc-list {
      list-style: none;
      padding: 0;
      margin: 0;
    }
    .toc-item {
      margin: 4px 0;
    }
    .toc-link {
      color: #71717a;
      text-decoration: none;
      display: block;
      padding: 2px 0;
      border-left: 2px solid transparent;
      padding-left: 8px;
      transition: all 0.15s;
    }
    .toc-link:hover {
      color: #a1a1aa;
      border-left-color: #3b82f6;
    }
    .toc-h3 { padding-left: 16px; font-size: 10px; }
    .toc-h4 { padding-left: 24px; font-size: 10px; }
    @media (max-width: 1200px) {
      .toc-sidebar { display: none; }
    }
  </style>
</head>
<body class="p-8 md:p-16">
  <div class="status" id="status">Updated</div>
  <div class="processing" id="processing"></div>
  <nav class="toc-sidebar" id="toc"></nav>
  <article class="prose max-w-2xl mx-auto" id="content">
    <div class="empty">Select a file in Dispatch</div>
  </article>
  <div class="filename" id="filename"></div>
  <script>
    var lastContent = '';
    var polling = false;

    function renderToc(items, level) {
      level = level || 0;
      if (!items || items.length === 0) return '';
      var html = level === 0 ? '<div class="toc-title">Contents</div>' : '';
      html += '<ul class="toc-list">';
      items.forEach(function(item) {
        var levelClass = 'toc-' + item.level;
        html += '<li class="toc-item">';
        html += '<a href="#' + item.slug + '" class="toc-link ' + levelClass + '">' + item.text + '</a>';
        if (item.children && item.children.length > 0) {
          html += renderToc(item.children, level + 1);
        }
        html += '</li>';
      });
      html += '</ul>';
      return html;
    }

    async function poll() {
      if (polling) return;
      polling = true;

      try {
        const res = await fetch('/content');
        const data = await res.json();

        document.getElementById('filename').textContent = data.filename || '';
        document.getElementById('processing').textContent = data.processing ? 'Processing...' : '';

        if (data.html && data.html !== lastContent) {
          lastContent = data.html;
          document.getElementById('content').innerHTML = data.html;
          // Render TOC
          if (data.toc && data.toc.length > 0) {
            document.getElementById('toc').innerHTML = renderToc(data.toc);
          } else {
            document.getElementById('toc').innerHTML = '';
          }
          var s = document.getElementById('status');
          s.classList.add('show');
          setTimeout(function() { s.classList.remove('show'); }, 1000);
        } else if (!data.html && !data.error && !data.processing) {
          document.getElementById('content').innerHTML = '<div class="empty">Select a file in Dispatch</div>';
        } else if (data.error) {
          document.getElementById('content').innerHTML = '<div class="empty" style="color:#ef4444">Error: ' + data.error + '</div>';
        }
      } catch(e) {
        console.error(e);
      }

      polling = false;
    }

    setInterval(poll, 400);
    setTimeout(poll, 100);
  </script>
</body>
</html>`

let processing = false
let cachedHtml = ''
let cachedToc = []
let cachedFile = ''

const server = createServer(async (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*')
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type')

  if (req.method === 'OPTIONS') {
    res.writeHead(204)
    res.end()
    return
  }

  const url = new URL(req.url, `http://localhost:${PORT}`)

  if (url.pathname === '/set-file' && req.method === 'POST') {
    let body = ''
    req.on('data', chunk => body += chunk)
    req.on('end', async () => {
      try {
        const data = JSON.parse(body)
        const newFile = data.path || ''

        if (newFile && newFile !== currentFile) {
          currentFile = newFile
          cachedHtml = ''
          console.log(`Preview: ${path.basename(currentFile)}`)

          // Start processing in background
          processing = true
          processWithWebsite2(currentFile)
            .then(result => {
              cachedHtml = result.html
              cachedToc = result.toc || []
              cachedFile = currentFile
              processing = false
              console.log(`Ready: ${path.basename(currentFile)}`)
            })
            .catch(err => {
              console.error('Error:', err.message)
              processing = false
            })
        }

        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
      } catch (e) {
        res.writeHead(400)
        res.end(JSON.stringify({ error: e.message }))
      }
    })
    return
  }

  if (url.pathname === '/content') {
    const filename = currentFile ? path.basename(currentFile) : ''

    // If file changed and we're still processing, re-process
    if (currentFile && currentFile !== cachedFile && !processing) {
      processing = true
      processWithWebsite2(currentFile)
        .then(result => {
          cachedHtml = result.html
          cachedToc = result.toc || []
          cachedFile = currentFile
          processing = false
        })
        .catch(err => {
          console.error('Error:', err.message)
          processing = false
        })
    }

    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({
      html: cachedHtml,
      toc: cachedToc,
      filename,
      processing
    }))
    return
  }

  res.writeHead(200, { 'Content-Type': 'text/html' })
  res.end(HTML)
})

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n✨ Preview server: http://127.0.0.1:${PORT}`)
  console.log(`📦 Using EXACT pipeline from: ${WEBSITE2_PATH}`)
  console.log(`\nSelect a file in Dispatch to preview.\n`)
})
