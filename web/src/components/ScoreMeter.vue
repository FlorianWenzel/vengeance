<script setup lang="ts">
// Horizontal meter for a 0..1 score. Text label always accompanies the bar.
import { computed } from "vue";
const props = withDefaults(
  defineProps<{ score: number; size?: "sm" | "lg"; label?: string }>(),
  { size: "sm" },
);
const pctv = computed(() => Math.max(0, Math.min(1, props.score || 0)) * 100);
</script>

<template>
  <div
    role="meter"
    :aria-valuenow="Math.round(pctv)"
    aria-valuemin="0"
    aria-valuemax="100"
    :aria-label="label ?? 'Coverage'"
    class="relative w-full overflow-hidden rounded-[2px] bg-ink-700"
    :class="size === 'lg' ? 'h-3' : 'h-1.5'"
  >
    <div
      class="h-full rounded-[2px] bg-beam transition-[width] duration-500"
      :style="{ width: `${pctv}%` }"
    />
    <template v-if="size === 'lg'">
      <span
        v-for="t in [25, 50, 75]"
        :key="t"
        class="absolute inset-y-0 w-px bg-ink-900/70"
        :style="{ left: `${t}%` }"
      />
    </template>
  </div>
</template>
