<script setup lang="ts">
import AppIcon from './AppIcon.vue'
import SearchBeam from './SearchBeam.vue'
import type { IconName } from '../lib/icons'

withDefaults(defineProps<{ icon?: IconName; title: string; compact?: boolean }>(), { icon: 'moon', compact: false })
</script>

<template>
  <div
    class="relative isolate overflow-hidden rounded-md border border-dashed border-ink-600 text-center"
    :class="compact ? 'px-6 py-8' : 'px-6 py-14'"
    data-testid="empty-state"
  >
    <SearchBeam v-if="!compact" :angle="-18" :intensity="0.7" class="-z-10" />
    <div class="mx-auto mb-3 grid h-10 w-10 place-items-center rounded-full border border-ink-600 bg-ink-900 text-beam">
      <AppIcon :name="icon" :size="18" />
    </div>
    <h3 class="text-xl text-fog-100">{{ title }}</h3>
    <div class="mx-auto mt-2 max-w-md text-sm text-fog-300">
      <slot />
    </div>
    <div v-if="$slots.actions" class="mt-5 flex flex-wrap justify-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
