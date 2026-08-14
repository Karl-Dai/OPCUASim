// Pure UI helpers for the OPC UA master. No Tauri imports and no i18n here —
// components translate labels and keep these functions side-effect free.

const QUALITY_COLORS: Record<string, string> = {
  good: 'var(--c-green)',
  bad: 'var(--c-red)',
  uncertain: 'var(--c-yellow)',
  default: 'var(--c-subtext0)',
}

export function qualityColor(quality: string | null | undefined): string {
  const q = quality ?? ''
  if (q === '') return 'var(--c-overlay0)'
  if (q.startsWith('Good')) return QUALITY_COLORS.good
  if (q.startsWith('Bad') || q.includes('Error')) return QUALITY_COLORS.bad
  if (q.startsWith('Uncertain')) return QUALITY_COLORS.uncertain
  return QUALITY_COLORS.default
}

export function nodeIcon(nodeClass: string): string {
  switch (nodeClass) {
    case 'Method':
      return '⚙'
    case 'Object':
      return '📁'
    case 'Variable':
      return '🔢'
    default:
      return '•'
  }
}

/** Render the OPC UA access-level bitmask as "R · W" / "R" / "W" / hex. */
export function accessString(level: number): string {
  const parts: string[] = []
  if (level & 0x01) parts.push('R')
  if (level & 0x02) parts.push('W')
  if (parts.length === 0) return `0x${level.toString(16).padStart(2, '0')}`
  return parts.join(' · ')
}

export function isWritable(level: number): boolean {
  return (level & 0x02) !== 0
}

/**
 * Timestamp abbreviation matching the legacy egui `format_hms`: show the local
 * HH:MM:SS slice when the string is long enough, otherwise the raw value.
 */
export function formatHms(ts: string | null | undefined): string {
  if (!ts) return '—'
  if (ts.length >= 19) return ts.slice(11, 19)
  return ts
}

/** Millisecond timestamp → local HH:MM:SS.mmm (legacy log panel format). */
export function formatTimestampMs(ms: number): string {
  const d = new Date(ms)
  if (Number.isNaN(d.getTime())) return '—'
  const pad = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`
}

/** Detect complex values from `variant_to_display_string` output. */
export function isComplexValue(value: string): boolean {
  return value.startsWith('[') || value.length > 50
}

/** Truncate at a UTF-8 code point boundary. */
export function truncateSafe(value: string, max: number): string {
  if (value.length <= max) return value
  const chars = Array.from(value)
  if (chars.length <= max) return value
  return chars.slice(0, max).join('')
}

/** Chart-ready points from history rows; skips non-numeric/undated rows. */
export interface ChartPoint {
  x: number
  y: number
}

export function toChartPoints(points: { source_timestamp: string; numeric: number | null }[]): ChartPoint[] {
  const out: ChartPoint[] = []
  for (const p of points) {
    if (p.numeric === null || p.numeric === undefined) continue
    const t = Date.parse(p.source_timestamp)
    if (Number.isNaN(t)) continue
    out.push({ x: t, y: p.numeric })
  }
  return out
}
