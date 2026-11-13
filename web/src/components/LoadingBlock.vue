<script setup lang="ts">
withDefaults(defineProps<{ rows?: number; label?: string }>(), {
  rows: 5,
  label: "Loading",
});
</script>

<template>
  <div
    role="status"
    aria-live="polite"
    class="space-y-2 py-2"
    data-testid="loading"
  >
    <span class="sr-only">{{ label }}…</span>
    <div
      v-for="i in rows"
      :key="i"
      class="skeleton h-7 rounded-[3px]"
      :style="{ width: `${100 - ((i * 13) % 35)}%` }"
    />
  </div>
</template>

<style scoped>
.skeleton {
  background: linear-gradient(
    90deg,
    var(--color-ink-800) 0%,
    var(--color-ink-750) 40%,
    var(--color-ink-800) 80%
  );
  background-size: 200% 100%;
  animation: shimmer 1.6s linear infinite;
}
@keyframes shimmer {
  from {
    background-position: 100% 0;
  }
  to {
    background-position: -100% 0;
  }
}
</style>
