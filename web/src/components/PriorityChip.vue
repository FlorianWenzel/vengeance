<script setup lang="ts">
import { computed } from "vue";
import type { Priority } from "../lib/types";
import { priorityHint } from "../lib/labels";

const props = withDefaults(defineProps<{ p: Priority; size?: "sm" | "md" }>(), {
  size: "sm",
});
const color = computed(() => `var(--color-p${props.p})`);
</script>

<template>
  <span
    class="pchip num"
    :class="size === 'md' ? 'pchip-md' : ''"
    :style="{ '--pc': color }"
    :title="priorityHint[p]"
    >P{{ p }}</span
  >
</template>

<style scoped>
.pchip {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  height: 1.25rem;
  padding: 0 0.4rem 0 0.35rem;
  border-radius: 3px;
  font-family: var(--font-mono);
  font-size: 0.72rem;
  font-weight: 700;
  color: var(--pc);
  background: color-mix(in oklab, var(--pc) 13%, transparent);
  border: 1px solid color-mix(in oklab, var(--pc) 45%, transparent);
  white-space: nowrap;
}
.pchip::before {
  content: "";
  width: 0.35rem;
  height: 0.35rem;
  border-radius: 1px;
  background: var(--pc);
}
.pchip-md {
  height: 1.5rem;
  font-size: 0.8rem;
  padding: 0 0.5rem 0 0.45rem;
}
</style>
