<script setup lang="ts">
import type { State } from "../lib/types";
import { stateLabel } from "../lib/labels";
defineProps<{ state: State }>();
const tone: Record<State, string> = {
  new: "text-beam border-beam/50",
  acknowledged: "text-fog-200 border-ink-600",
  accepted: "text-fog-300 border-ink-600 border-dashed",
  false_positive: "text-fog-400 border-ink-600 border-dashed",
  fixed: "text-ok border-ok/40",
};
</script>

<template>
  <span
    class="inline-flex h-5 items-center rounded-[3px] border px-1.5 text-[0.7rem] font-medium whitespace-nowrap"
    :class="tone[state]"
  >
    {{ stateLabel[state] }}
  </span>
</template>
