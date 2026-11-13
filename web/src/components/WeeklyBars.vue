<script setup lang="ts">
// New vs fixed per ISO week: paired bars, 2px gap, rounded data-ends on the baseline.
import { computed, ref } from "vue";
import { niceTicks, useWidth } from "../lib/useWidth";

const props = defineProps<{
  weeks: { week: string; new: number; fixed: number }[];
}>();

const wrap = ref<HTMLElement | null>(null);
const width = useWidth(wrap);
const H = 190;
const pad = { t: 10, r: 4, b: 22, l: 30 };
const hover = ref<number | null>(null);

const scale = computed(() =>
  niceTicks(Math.max(0, ...props.weeks.flatMap((w) => [w.new, w.fixed]))),
);
const innerW = computed(() => Math.max(10, width.value - pad.l - pad.r));
const innerH = H - pad.t - pad.b;
const band = computed(() => innerW.value / Math.max(1, props.weeks.length));
const barW = computed(() => Math.max(3, Math.min(14, (band.value - 10) / 2)));

function y(v: number) {
  return pad.t + innerH - (v / scale.value.top) * innerH;
}
function bar(xLeft: number, v: number) {
  const top = y(v);
  const base = y(0);
  const h = base - top;
  if (h <= 0) return "";
  const w = barW.value;
  const r = Math.min(3, w / 2, h);
  return `M${xLeft},${base}V${top + r}Q${xLeft},${top} ${xLeft + r},${top}H${xLeft + w - r}Q${xLeft + w},${top} ${xLeft + w},${top + r}V${base}Z`;
}
function cx(i: number) {
  return pad.l + band.value * i + band.value / 2;
}
function weekLabel(w: string) {
  const m = /W(\d+)$/.exec(w);
  return m ? `W${m[1]}` : w;
}
function onKey(e: KeyboardEvent) {
  const n = props.weeks.length;
  if (!n || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
  e.preventDefault();
  const cur = hover.value ?? n - 1;
  hover.value = Math.min(
    n - 1,
    Math.max(0, cur + (e.key === "ArrowLeft" ? -1 : 1)),
  );
}
const tip = computed(() => {
  if (hover.value === null) return null;
  const w = props.weeks[hover.value];
  return w
    ? { w, left: cx(hover.value), flip: cx(hover.value) > width.value - 130 }
    : null;
});
</script>

<template>
  <div ref="wrap" class="relative w-full min-w-0 overflow-hidden">
    <div class="mb-2 flex items-center gap-4 text-xs text-fog-300">
      <span class="inline-flex items-center gap-1.5"
        ><span class="h-2.5 w-2.5 rounded-[2px] bg-series-new" />New</span
      >
      <span class="inline-flex items-center gap-1.5"
        ><span class="h-2.5 w-2.5 rounded-[2px] bg-series-fixed" />Fixed</span
      >
    </div>
    <svg
      :width="width"
      :height="H"
      class="block select-none"
      role="img"
      tabindex="0"
      :aria-label="`New versus fixed findings for the last ${weeks.length} weeks. Use arrow keys to inspect.`"
      @pointerleave="hover = null"
      @keydown="onKey"
      @blur="hover = null"
    >
      <template v-for="t in scale.ticks" :key="t">
        <line
          :x1="pad.l"
          :x2="width - pad.r"
          :y1="y(t)"
          :y2="y(t)"
          stroke="var(--color-ink-700)"
          :stroke-dasharray="t === 0 ? undefined : '2 4'"
        />
        <text
          :x="pad.l - 8"
          :y="y(t) + 4"
          text-anchor="end"
          class="num"
          fill="var(--color-fog-500)"
          font-size="10.5"
        >
          {{ t }}
        </text>
      </template>
      <g v-for="(w, i) in weeks" :key="w.week">
        <rect
          :x="pad.l + band * i"
          :y="pad.t"
          :width="band"
          :height="innerH"
          :fill="hover === i ? 'var(--color-ink-800)' : 'transparent'"
          @pointerenter="hover = i"
        />
        <path
          :d="bar(cx(i) - barW - 1, w.new)"
          fill="var(--color-series-new)"
          pointer-events="none"
        />
        <path
          :d="bar(cx(i) + 1, w.fixed)"
          fill="var(--color-series-fixed)"
          pointer-events="none"
        />
        <text
          v-if="weeks.length <= 8 || i % 2 === (weeks.length - 1) % 2"
          :x="cx(i)"
          :y="H - 5"
          text-anchor="middle"
          fill="var(--color-fog-500)"
          font-size="10.5"
        >
          {{ weekLabel(w.week) }}
        </text>
      </g>
    </svg>
    <div
      v-if="tip"
      class="pointer-events-none absolute top-6 rounded-[4px] border border-ink-600 bg-ink-800 px-2.5 py-1.5 text-xs shadow-lg"
      :style="
        tip.flip
          ? { right: `${width - tip.left + 14}px` }
          : { left: `${tip.left + 14}px` }
      "
    >
      <div class="text-fog-400">{{ tip.w.week }}</div>
      <div class="num flex items-center gap-1.5 text-fog-100">
        <span class="h-2 w-2 rounded-[2px] bg-series-new" />{{ tip.w.new }} new
      </div>
      <div class="num flex items-center gap-1.5 text-fog-100">
        <span class="h-2 w-2 rounded-[2px] bg-series-fixed" />{{ tip.w.fixed }}
        fixed
      </div>
    </div>
    <table class="sr-only">
      <caption>
        New and fixed findings per week
      </caption>
      <tr>
        <th>Week</th>
        <th>New</th>
        <th>Fixed</th>
      </tr>
      <tr v-for="w in weeks" :key="w.week">
        <td>{{ w.week }}</td>
        <td>{{ w.new }}</td>
        <td>{{ w.fixed }}</td>
      </tr>
    </table>
  </div>
</template>
