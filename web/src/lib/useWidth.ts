import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

/** Tracks an element's content width (for SVG charts that draw in real pixels). */
export function useWidth(el: Ref<HTMLElement | null>, fallback = 600) {
  const width = ref(fallback)
  let ro: ResizeObserver | null = null
  onMounted(() => {
    if (!el.value) return
    width.value = el.value.clientWidth || fallback
    ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width
      if (w) width.value = Math.floor(w)
    })
    ro.observe(el.value)
  })
  onBeforeUnmount(() => ro?.disconnect())
  return width
}

/** "Nice" upper bound and ticks for a 0-based linear axis. */
export function niceTicks(max: number, count = 4): { top: number; ticks: number[] } {
  if (!(max > 0)) return { top: 4, ticks: [0, 1, 2, 3, 4] }
  const raw = max / count
  const mag = 10 ** Math.floor(Math.log10(raw))
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw
  const s = step < 1 ? 1 : Math.ceil(step)
  const top = Math.ceil(max / s) * s
  const ticks: number[] = []
  for (let v = 0; v <= top + 1e-9; v += s) ticks.push(v)
  return { top, ticks }
}
