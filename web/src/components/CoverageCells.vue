<script setup lang="ts">
import AppIcon from './AppIcon.vue'
import { ago, dateTime } from '../lib/format'
import { scanTypeLabel } from '../lib/labels'
import type { CoverageCell } from '../lib/types'

withDefaults(defineProps<{ cells: CoverageCell[]; stacked?: boolean }>(), { stacked: false })

function tip(c: CoverageCell) {
  const exp = `Expected every ${c.maxAgeDays} ${c.maxAgeDays === 1 ? 'day' : 'days'}.`
  if (!c.lastAt) return `${scanTypeLabel[c.scanType]}: never scanned. ${exp}`
  return `${scanTypeLabel[c.scanType]}: last ${dateTime(c.lastAt)}. ${exp}`
}
</script>

<template>
  <span v-if="!cells.length" class="text-xs text-fog-500">no expectations</span>
  <ul v-else class="flex gap-1.5" :class="stacked ? 'flex-col' : 'flex-wrap'">
    <li
      v-for="c in cells"
      :key="c.scanType"
      class="covcell"
      :class="c.ok ? 'is-ok' : 'is-gap'"
      data-testid="coverage-cell"
      :data-ok="c.ok ? 'true' : 'false'"
      :data-scan-type="c.scanType"
      :title="tip(c)"
    >
      <AppIcon :name="c.ok ? 'check' : 'triangle-alert'" :size="11" />
      <span class="font-medium">{{ scanTypeLabel[c.scanType] }}</span>
      <span v-if="!c.ok" class="opacity-90">{{ c.lastAt ? `last ${ago(c.lastAt)}` : 'never' }}</span>
      <span class="sr-only">{{ c.ok ? 'fresh' : 'gap' }}</span>
    </li>
  </ul>
</template>

<style scoped>
.covcell {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  height: 1.3rem;
  padding: 0 0.4rem;
  border-radius: 3px;
  font-size: 0.7rem;
  white-space: nowrap;
  border: 1px solid;
}
.is-ok {
  color: var(--color-fog-300);
  border-color: var(--color-ink-600);
}
.is-ok :deep(svg) {
  color: var(--color-ok);
}
.is-gap {
  color: var(--color-warn);
  border-color: color-mix(in oklab, var(--color-warn) 50%, transparent);
  background: color-mix(in oklab, var(--color-warn) 8%, transparent);
}
</style>
