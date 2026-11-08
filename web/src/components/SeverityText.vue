<script setup lang="ts">
import type { Severity } from '../lib/types'
defineProps<{ severity: Severity }>()
const bars: Record<Severity, number> = { critical: 4, high: 3, medium: 2, low: 1, unknown: 0 }
</script>

<template>
  <span class="inline-flex items-center gap-1.5 whitespace-nowrap text-fog-200">
    <span class="inline-flex items-end gap-px" aria-hidden="true">
      <span
        v-for="i in 4"
        :key="i"
        class="w-[3px] rounded-[1px]"
        :class="i <= bars[severity] ? 'bg-fog-200' : 'bg-ink-600'"
        :style="{ height: `${3 + i * 2}px` }"
      />
    </span>
    <span class="capitalize">{{ severity }}</span>
  </span>
</template>
