const DAY = 86_400_000

/** "just now", "5 min ago", "3 h ago", "9 days ago"; "never" for null. */
export function ago(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return 'never'
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return '—'
  const diff = now - t
  if (diff < 0) return 'in ' + span(-diff)
  if (diff < 60_000) return 'just now'
  return span(diff) + ' ago'
}

function span(ms: number): string {
  const min = Math.round(ms / 60_000)
  if (min < 60) return `${min} min`
  const h = Math.round(ms / 3_600_000)
  if (h < 48) return `${h} h`
  const d = Math.round(ms / DAY)
  if (d < 60) return `${d} ${d === 1 ? 'day' : 'days'}`
  const mo = Math.round(d / 30)
  if (mo < 24) return `${mo} months`
  return `${Math.round(d / 365)} years`
}

export function dateTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '—'
  return d.toLocaleString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

export function date(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso.length === 10 ? iso + 'T00:00:00Z' : iso)
  if (Number.isNaN(d.getTime())) return '—'
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', timeZone: iso.length === 10 ? 'UTC' : undefined })
}

/** YYYY-MM-DD for a date `days` from today (local). */
export function isoDay(days = 0): string {
  const d = new Date(Date.now() + days * DAY)
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${m}-${day}`
}

export function pct(score: number | null | undefined): string {
  if (score === null || score === undefined || Number.isNaN(score)) return '—'
  return `${Math.round(score * 100)}%`
}

export function num(n: number | null | undefined): string {
  if (n === null || n === undefined) return '—'
  return n.toLocaleString()
}

export function plural(n: number, one: string, many = one + 's'): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`
}

export function parseTags(s: string): string[] {
  return Array.from(new Set(s.split(',').map((t) => t.trim()).filter(Boolean)))
}
