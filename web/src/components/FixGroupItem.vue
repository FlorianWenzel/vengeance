<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import PriorityChip from './PriorityChip.vue'
import CommandBlock from './CommandBlock.vue'
import type { FixGroup } from '../lib/types'

const props = withDefaults(defineProps<{ group: FixGroup; showAsset?: boolean; maxAdvisories?: number }>(), {
  showAsset: false,
  maxAdvisories: 4,
})
const shown = computed(() => props.group.advisories.slice(0, props.maxAdvisories))
const more = computed(() => Math.max(0, props.group.advisories.length - props.maxAdvisories))
const findingsLink = computed(() => ({
  path: '/findings',
  query: { asset: String(props.group.assetId), q: props.group.pkgName },
}))
</script>

<template>
  <li class="py-3.5" data-testid="fix-group" :data-key="group.key">
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      <PriorityChip :p="group.maxPriority" />
      <span v-if="group.kev" class="inline-flex items-center gap-0.5 rounded-[3px] border border-p1/60 px-1 font-mono text-[0.62rem] font-bold text-[#ff8a99]" title="Fixes a known exploited vulnerability (CISA KEV)">
        <AppIcon name="flame" :size="10" />KEV
      </span>
      <p class="min-w-0 font-mono text-[0.85rem]">
        <span class="font-semibold text-fog-100">{{ group.pkgName }}</span>
        <span class="ml-2 text-fog-400">{{ group.installedVersion }}</span>
        <span class="mx-1.5 text-fog-500">→</span>
        <span class="font-semibold text-ok">{{ group.targetVersion }}</span>
      </p>
      <span class="tag">{{ group.ecosystem }}</span>
      <RouterLink :to="findingsLink" class="ml-auto text-xs whitespace-nowrap text-fog-400 hover:text-fog-100">
        fixes <span class="num font-semibold text-fog-100">{{ group.findings }}</span> {{ group.findings === 1 ? 'finding' : 'findings' }}
      </RouterLink>
    </div>
    <p v-if="showAsset" class="mt-1 text-xs text-fog-400">
      in <RouterLink :to="`/assets/${group.assetId}`" class="text-fog-200 hover:text-beam">{{ group.assetName }}</RouterLink>
    </p>
    <p class="mt-1 flex flex-wrap gap-x-2 font-mono text-[0.7rem] text-fog-500">
      <span v-for="a in shown" :key="a">{{ a }}</span>
      <span v-if="more">+{{ more }} more</span>
    </p>
    <CommandBlock :command="group.command" testid="fix-command" class="mt-2" />
  </li>
</template>
