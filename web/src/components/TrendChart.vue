<script setup lang="ts">
// 90-day open-findings trend: single-series area. Crosshair + tooltip on hover/keyboard.
import { computed, ref } from 'vue'
import { niceTicks, useWidth } from '../lib/useWidth'
import { date } from '../lib/format'

const props = defineProps<{ points: { date: string; open: number }[] }>()

const wrap = ref<HTMLElement | null>(null)
const width = useWidth(wrap)
const H = 190
const pad = { t: 10, r: 8, b: 22, l: 34 }
const hover = ref<number | null>(null)

const scale = computed(() => {
  const max = Math.max(0, ...props.points.map((p) => p.open))
  return niceTicks(max)
})
const innerW = computed(() => Math.max(10, width.value - pad.l - pad.r))
const innerH = H - pad.t - pad.b

function x(i: number) {
  const n = props.points.length
  return pad.l + (n <= 1 ? innerW.value / 2 : (i / (n - 1)) * innerW.value)
}
function y(v: number) {
  return pad.t + innerH - (v / scale.value.top) * innerH
}

const line = computed(() => props.points.map((p, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)},${y(p.open).toFixed(1)}`).join(''))
const area = computed(() => {
  if (!props.points.length) return ''
  const n = props.points.length
  return `${line.value}L${x(n - 1).toFixed(1)},${y(0)}L${x(0).toFixed(1)},${y(0)}Z`
})

const xLabels = computed(() => {
  const n = props.points.length
  if (!n) return []
  const idx = n < 3 ? [0, n - 1] : [0, Math.floor((n - 1) / 3), Math.floor(((n - 1) * 2) / 3), n - 1]
  return Array.from(new Set(idx)).map((i) => ({ i, label: shortDate(props.points[i].date) }))
})

function shortDate(d: string) {
  const dt = new Date(d + 'T00:00:00Z')
  return dt.toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' })
}

function onMove(e: PointerEvent) {
  const svg = e.currentTarget as SVGSVGElement
  const r = svg.getBoundingClientRect()
  const px = e.clientX - r.left
  const n = props.points.length
  if (!n) return
  const i = Math.round(((px - pad.l) / innerW.value) * (n - 1))
  hover.value = Math.min(n - 1, Math.max(0, i))
}
function onKey(e: KeyboardEvent) {
  const n = props.points.length
  if (!n) return
  if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    e.preventDefault()
    const cur = hover.value ?? n - 1
    hover.value = Math.min(n - 1, Math.max(0, cur + (e.key === 'ArrowLeft' ? -1 : 1)))
  }
}

const tip = computed(() => {
  if (hover.value === null) return null
  const p = props.points[hover.value]
  if (!p) return null
  const left = x(hover.value)
  return { p, left, flip: left > width.value - 140 }
})
const last = computed(() => props.points[props.points.length - 1])
</script>

<template>
  <div ref="wrap" class="relative w-full min-w-0 overflow-hidden">
    <svg
      :width="width"
      :height="H"
      class="block touch-none select-none"
      role="img"
      tabindex="0"
      :aria-label="`Open findings over the last ${points.length} days${last ? `, ${last.open} today` : ''}. Use arrow keys to inspect.`"
      @pointermove="onMove"
      @pointerleave="hover = null"
      @keydown="onKey"
      @blur="hover = null"
    >
      <g>
        <template v-for="t in scale.ticks" :key="t">
          <line :x1="pad.l" :x2="width - pad.r" :y1="y(t)" :y2="y(t)" stroke="var(--color-ink-700)" :stroke-dasharray="t === 0 ? undefined : '2 4'" />
          <text :x="pad.l - 8" :y="y(t) + 4" text-anchor="end" class="num" fill="var(--color-fog-500)" font-size="10.5">{{ t }}</text>
        </template>
        <text v-for="l in xLabels" :key="l.i" :x="x(l.i)" :y="H - 5" :text-anchor="l.i === 0 ? 'start' : l.i === points.length - 1 ? 'end' : 'middle'" fill="var(--color-fog-500)" font-size="10.5">{{ l.label }}</text>
      </g>
      <path :d="area" fill="var(--color-beam)" fill-opacity="0.1" />
      <path :d="line" fill="none" stroke="var(--color-beam)" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
      <g v-if="tip">
        <line :x1="tip.left" :x2="tip.left" :y1="pad.t" :y2="y(0)" stroke="var(--color-fog-400)" stroke-width="1" />
        <circle :cx="tip.left" :cy="y(tip.p.open)" r="4.5" fill="var(--color-beam)" stroke="var(--color-ink-850)" stroke-width="2" />
      </g>
      <circle v-else-if="last" :cx="x(points.length - 1)" :cy="y(last.open)" r="4" fill="var(--color-beam)" stroke="var(--color-ink-850)" stroke-width="2" />
    </svg>
    <div
      v-if="tip"
      class="pointer-events-none absolute top-1 rounded-[4px] border border-ink-600 bg-ink-800 px-2.5 py-1.5 text-xs shadow-lg"
      :style="tip.flip ? { right: `${width - tip.left + 10}px` } : { left: `${tip.left + 10}px` }"
    >
      <div class="text-fog-400">{{ date(tip.p.date) }}</div>
      <div class="num font-medium text-fog-100">{{ tip.p.open }} open</div>
    </div>
    <table class="sr-only">
      <caption>Open findings per day</caption>
      <tr><th>Date</th><th>Open</th></tr>
      <tr v-for="p in points" :key="p.date"><td>{{ p.date }}</td><td>{{ p.open }}</td></tr>
    </table>
  </div>
</template>
