<script setup lang="ts">
import { computed } from 'vue'
import type { OpenCounts, Priority } from '../lib/types'
const props = defineProps<{ open: OpenCounts }>()
const parts = computed(() =>
  ([1, 2, 3, 4] as Priority[]).map((p) => ({ p, n: props.open[`p${p}` as 'p1'] })).filter((x) => x.n > 0),
)
</script>

<template>
  <span v-if="!open.total" class="text-xs text-fog-500">none</span>
  <span v-else class="inline-flex flex-wrap items-center gap-x-2 gap-y-0.5 font-mono text-[0.72rem] num" :aria-label="`${open.total} open`">
    <span v-for="x in parts" :key="x.p" class="inline-flex items-center gap-1 whitespace-nowrap">
      <span class="font-bold" :style="{ color: `var(--color-p${x.p})` }">P{{ x.p }}</span><span class="text-fog-100">{{ x.n }}</span>
    </span>
  </span>
</template>
