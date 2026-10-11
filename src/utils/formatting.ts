/**
 * The one place a date from frontmatter, the journal or Rust becomes a Date.
 * Returns null instead of an Invalid Date, so callers can show nothing (or a
 * fallback) rather than the literal text "Invalid Date".
 *
 * Accepts a Date, epoch seconds, or a string in any shape the vault has used:
 * ISO with or without zone, a bare `2019-09-20`, YAML-quoted `'2019-…'`, and
 * Jekyll-style `2019-09-20 21:19:39 -0400` (which WebKit refuses to parse).
 */
export function parseDate(input: unknown): Date | null {
  if (input == null || input === '') return null
  if (input instanceof Date) return Number.isNaN(input.getTime()) ? null : input
  if (typeof input === 'number') {
    const d = new Date(input * 1000)
    return Number.isFinite(input) && !Number.isNaN(d.getTime()) ? d : null
  }
  if (typeof input !== 'string') return null
  let s = input.trim().replace(/^(['"])(.*)\1$/, '$2').trim()
  if (!s) return null
  // A bare date is a calendar day: pin it to local noon so it doesn't render
  // as the previous day west of UTC.
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) s += 'T12:00:00'
  // `2019-09-20 21:19:39 -0400` / `2019-09-20 21:19:39` -> ISO 8601.
  const m = s.match(/^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?)\s*(Z|[+-]\d{2}:?\d{2})?$/)
  if (m) {
    const zone = m[3] ? (m[3] === 'Z' ? 'Z' : m[3].replace(/^([+-]\d{2}):?(\d{2})$/, '$1:$2')) : ''
    s = `${m[1]}T${m[2]}${zone}`
  }
  const d = new Date(s)
  return Number.isNaN(d.getTime()) ? null : d
}

/** Epoch seconds, or null when the input isn't a usable date. */
export function parseEpochSeconds(input: unknown): number | null {
  const d = parseDate(input)
  return d ? Math.floor(d.getTime() / 1000) : null
}

export function formatDate(input: unknown): string {
  const d = parseDate(input)
  return d ? d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }) : ''
}

export function formatAgeShort(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return 'now'
  if (minutes < 60) return `${minutes}m ago`
  if (hours < 24) return `${hours}h ago`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d ago`
  if (days < 30) return `${Math.floor(days / 7)}w ago`
  if (days < 365) return `${Math.floor(days / 30)}mo ago`
  return `${Math.floor(days / 365)}y ago`
}

export function formatDateCompact(ts: number): string {
  return formatDate(ts)
}

export function formatScheduledTime(input: unknown): string {
  const d = parseDate(input)
  if (!d) return ''
  return d.toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
}
