<script setup lang="ts">
import { computed, onMounted } from 'vue'
import AppIcon from '../components/AppIcon.vue'
import PageHeader from '../components/PageHeader.vue'
import PriorityChip from '../components/PriorityChip.vue'
import TrendChart from '../components/TrendChart.vue'
import WeeklyBars from '../components/WeeklyBars.vue'
import ScoreMeter from '../components/ScoreMeter.vue'
import FixGroupItem from '../components/FixGroupItem.vue'
import EmptyState from '../components/EmptyState.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import { api } from '../lib/api'
import { useAsync } from '../lib/useAsync'
import { num, pct, plural } from '../lib/format'
import { PRIORITIES, priorityHint } from '../lib/labels'
import type { Overview, Priority } from '../lib/types'

const ov = useAsync(() => api.get<Overview>('/api/v1/overview'))
onMounted(() => ov.run())
const d = computed(() => ov.data.value)

function count(p: Priority) {
  const o = d.value?.open
  if (!o) return 0
  return o[`p${p}` as 'p1' | 'p2' | 'p3' | 'p4']
}

const trendDelta = computed(() => {
  const t = d.value?.trend ?? []
  if (t.length < 2) return null
  const first = t[Math.max(0, t.length - 31)].open
  const last = t[t.length - 1].open
  return last - first
})
const weeklyTotals = computed(() => {
  const w = d.value?.weekly ?? []
  return { new: w.reduce((s, x) => s + x.new, 0), fixed: w.reduce((s, x) => s + x.fixed, 0) }
})
const today = new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })
</script>

<template>
  <div>
    <PageHeader title="Overview" :eyebrow="today">
      <template #actions>
        <button type="button" class="btn btn-ghost" :disabled="ov.loading.value" @click="ov.run()">
          <AppIcon name="refresh-cw" :size="14" :class="ov.loading.value ? 'animate-spin' : ''" />Refresh
        </button>
      </template>
    </PageHeader>

    <LoadingBlock v-if="ov.loading.value && !d" :rows="6" label="Loading overview" />
    <ErrorBlock v-else-if="ov.error.value && !d" :message="ov.error.value" @retry="ov.run()" />

    <template v-else-if="d">
      <RouterLink
        v-if="d.staleSources > 0"
        to="/sources"
        class="mb-5 flex items-center gap-3 rounded-md border border-warn/50 bg-warn/[0.07] px-4 py-2.5 text-sm hover:border-warn"
        role="alert"
        data-testid="stale-warning"
      >
        <AppIcon name="triangle-alert" :size="16" class="text-warn" />
        <span class="text-fog-100"><span class="num font-semibold">{{ plural(d.staleSources, 'source') }}</span> went quiet. Results may be out of date.</span>
        <span class="ml-auto inline-flex items-center gap-1 text-xs text-fog-300">Check sources<AppIcon name="arrow-right" :size="12" /></span>
      </RouterLink>

      <!-- Priority strip -->
      <section aria-label="Open findings by priority" class="grid grid-cols-2 overflow-hidden rounded-md border border-ink-700 sm:grid-cols-3 lg:grid-cols-6">
        <RouterLink
          v-for="p in PRIORITIES"
          :key="p"
          :to="{ path: '/findings', query: { priority: String(p) } }"
          class="ptile group"
          :style="{ '--pc': `var(--color-p${p})` }"
          :title="priorityHint[p]"
        >
          <PriorityChip :p="p" />
          <span class="mt-3 block font-display text-[2.6rem] leading-none font-bold text-fog-100 num" :data-testid="`p${p}-count`">{{ count(p) }}</span>
          <span class="mt-1 block text-xs text-fog-500 group-hover:text-fog-300">open</span>
        </RouterLink>
        <RouterLink :to="{ path: '/findings', query: { priority: '1' } }" class="ptile" style="--pc: var(--color-p1)">
          <span class="inline-flex items-center gap-1 font-mono text-[0.72rem] font-bold text-[#ff8a99]"><AppIcon name="flame" :size="12" />KEV</span>
          <span class="mt-3 block font-display text-[2.6rem] leading-none font-bold text-fog-100 num" data-testid="kev-count">{{ d.kev }}</span>
          <span class="mt-1 block text-xs text-fog-500">known exploited</span>
        </RouterLink>
        <RouterLink to="/findings" class="ptile" style="--pc: var(--color-beam)">
          <span class="eyebrow !text-[0.7rem]">Total open</span>
          <span class="mt-3 block font-display text-[2.6rem] leading-none font-bold text-beam num" data-testid="open-total">{{ d.open.total }}</span>
          <span class="mt-1 block text-xs text-fog-500">new + acknowledged</span>
        </RouterLink>
      </section>

      <!-- Charts -->
      <div class="mt-8 grid grid-cols-1 gap-x-10 gap-y-8 lg:grid-cols-2">
        <section class="section min-w-0">
          <div class="mb-3 flex items-baseline justify-between gap-3">
            <h2 class="text-lg text-fog-100">Open findings, 90 days</h2>
            <p v-if="trendDelta !== null" class="num text-xs text-fog-400">
              <span :class="trendDelta > 0 ? 'text-[#ff8a99]' : trendDelta < 0 ? 'text-ok' : ''">{{ trendDelta > 0 ? '+' : '' }}{{ trendDelta }}</span>
              in 30 days
            </p>
          </div>
          <TrendChart v-if="d.trend.length" :points="d.trend" />
          <p v-else class="py-10 text-center text-sm text-fog-500">No history yet. The trend fills in day by day once scans arrive.</p>
        </section>
        <section class="section min-w-0">
          <div class="mb-1 flex items-baseline justify-between gap-3">
            <h2 class="text-lg text-fog-100">New vs fixed, 12 weeks</h2>
            <p class="num text-xs text-fog-400">{{ weeklyTotals.new }} new · {{ weeklyTotals.fixed }} fixed</p>
          </div>
          <WeeklyBars v-if="d.weekly.length" :weeks="d.weekly" />
          <p v-else class="py-10 text-center text-sm text-fog-500">No weekly data yet.</p>
        </section>
      </div>

      <!-- Queue + coverage -->
      <div class="mt-8 grid grid-cols-1 gap-x-10 gap-y-8 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <section class="section min-w-0">
          <div class="mb-1 flex items-baseline justify-between gap-3">
            <h2 class="text-lg text-fog-100">Fix this week</h2>
            <RouterLink to="/fixes" class="text-xs text-fog-400 hover:text-beam">All fixes <AppIcon name="arrow-right" :size="11" /></RouterLink>
          </div>
          <p class="text-xs text-fog-500">The upgrades that close the most urgent findings.</p>
          <ul v-if="d.queue.length" class="divide-y divide-ink-700">
            <FixGroupItem v-for="g in d.queue" :key="g.key" :group="g" show-asset :max-advisories="3" />
          </ul>
          <EmptyState v-else icon="wrench" title="Nothing to upgrade" compact class="mt-3">
            No open finding has a known fix right now. When an upgrade would close findings, it is queued here with a copy-ready command.
          </EmptyState>
        </section>

        <section class="section min-w-0">
          <div class="mb-3 flex items-baseline justify-between gap-3">
            <h2 class="text-lg text-fog-100">Coverage</h2>
            <RouterLink to="/coverage" class="text-xs text-fog-400 hover:text-beam">Details <AppIcon name="arrow-right" :size="11" /></RouterLink>
          </div>
          <template v-if="d.coverage.assets > 0">
            <p class="font-display text-[3.4rem] leading-none font-bold text-fog-100 num" data-testid="coverage-score">{{ pct(d.coverage.score) }}</p>
            <p class="mt-1 text-xs text-fog-400">of expected scans are fresh across {{ plural(d.coverage.assets, 'asset') }}</p>
            <ScoreMeter :score="d.coverage.score" size="lg" class="mt-4" />
            <RouterLink
              to="/coverage"
              class="mt-4 flex items-center gap-2 text-sm"
              :class="d.coverage.gaps ? 'text-warn hover:text-beam-hi' : 'text-ok'"
              data-testid="coverage-gaps"
            >
              <AppIcon :name="d.coverage.gaps ? 'triangle-alert' : 'circle-check'" :size="15" />
              <span v-if="d.coverage.gaps"><span class="num font-semibold">{{ num(d.coverage.gaps) }}</span> {{ d.coverage.gaps === 1 ? 'gap' : 'gaps' }}: scans missing or stale</span>
              <span v-else>No gaps. Every asset is scanned on time.</span>
            </RouterLink>
          </template>
          <template v-else>
            <p class="font-display text-[3.4rem] leading-none font-bold text-fog-500 num" data-testid="coverage-score">{{ pct(d.coverage.score) }}</p>
            <p class="mt-2 text-sm text-fog-400">
              No assets yet. <RouterLink to="/assets" class="link">Add assets</RouterLink> or push a scan, and coverage shows which ones are not being scanned.
            </p>
          </template>
        </section>
      </div>
    </template>
  </div>
</template>

<style scoped>
.ptile {
  display: block;
  padding: 1rem 1.1rem 0.9rem;
  border-right: 1px solid var(--color-ink-700);
  border-bottom: 1px solid var(--color-ink-700);
  margin: 0 -1px -1px 0;
  position: relative;
  transition: background-color 120ms;
}
.ptile::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  background: var(--pc);
  opacity: 0.55;
}
.ptile:hover {
  background: var(--color-ink-850);
}
.ptile:hover::after {
  opacity: 1;
}
</style>
