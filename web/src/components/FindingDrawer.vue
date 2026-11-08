<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import SideDrawer from './SideDrawer.vue'
import PriorityChip from './PriorityChip.vue'
import SeverityText from './SeverityText.vue'
import StateBadge from './StateBadge.vue'
import KevBadge from './KevBadge.vue'
import LoadingBlock from './LoadingBlock.vue'
import ErrorBlock from './ErrorBlock.vue'
import TriageControls from './TriageControls.vue'
import { api } from '../lib/api'
import { useAsync } from '../lib/useAsync'
import { ago, date, dateTime } from '../lib/format'
import { assetKindIcon, kindLabel, priorityHint, stateLabel } from '../lib/labels'
import type { FindingDetail } from '../lib/types'

const props = defineProps<{ id: number | null }>()
const emit = defineEmits<{ close: []; changed: [] }>()

const open = computed(() => props.id !== null)
const detail = useAsync(() => api.get<FindingDetail>(`/api/v1/findings/${props.id}`))
const f = computed(() => detail.data.value)

watch(
  () => props.id,
  (id) => {
    if (id !== null) {
      detail.data.value = null
      detail.run()
    }
  },
  { immediate: true },
)

function onTriaged() {
  detail.run()
  emit('changed')
}

function isLink(u: string) {
  return /^https?:\/\//i.test(u)
}
function host(u: string) {
  try {
    return new URL(u).host
  } catch {
    return u
  }
}

// Newest first, regardless of the order the API returns.
const events = computed(() => [...(f.value?.events ?? [])].sort((a, b) => Date.parse(b.at) - Date.parse(a.at)))

const descOpen = ref(false)
const longDesc = computed(() => (f.value?.description?.length ?? 0) > 600)
</script>

<template>
  <SideDrawer :open="open" label="Finding details" testid="finding-drawer" @close="emit('close')">
    <div class="flex items-center justify-between gap-3 border-b border-ink-700 px-5 py-3">
      <p class="eyebrow">Finding <span v-if="id" class="num text-fog-500">#{{ id }}</span></p>
      <button type="button" class="btn btn-ghost btn-icon -mr-2" aria-label="Close details" @click="emit('close')">
        <AppIcon name="x" :size="16" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto px-5 py-5">
      <LoadingBlock v-if="detail.loading.value && !f" :rows="6" label="Loading finding" />
      <ErrorBlock v-else-if="detail.error.value && !f" :message="detail.error.value" @retry="detail.run()" />
      <template v-else-if="f">
        <div class="flex flex-wrap items-center gap-2">
          <PriorityChip :p="f.priority" size="md" />
          <SeverityText :severity="f.severity" />
          <KevBadge v-if="f.kev" />
          <StateBadge :state="f.state" />
          <span v-if="f.cvss !== null" class="tag num">CVSS {{ f.cvss.toFixed(1) }}</span>
        </div>
        <p class="mt-1.5 text-xs text-fog-500">{{ priorityHint[f.priority] }}</p>

        <h2 class="mt-4 font-mono text-lg font-semibold tracking-normal break-all text-beam normal-case">{{ f.advisoryId }}</h2>
        <p class="mt-1 text-base text-fog-100">{{ f.title }}</p>

        <div class="mt-5">
          <TriageControls v-if="f.state !== 'fixed'" :ids="[f.id]" :state="f.state" test-prefix="drawer" size="md" @done="onTriaged" />
          <p v-else class="flex items-center gap-2 text-sm text-ok">
            <AppIcon name="circle-check" :size="15" />Fixed {{ ago(f.fixedAt) }}. It reopens automatically if a scan reports it again.
          </p>
          <p v-if="f.state === 'accepted' && f.acceptedUntil" class="mt-2 text-sm text-fog-300">
            Risk accepted until <span class="num text-fog-100">{{ date(f.acceptedUntil) }}</span><template v-if="f.stateReason">: {{ f.stateReason }}</template>
          </p>
          <p v-else-if="f.state === 'false_positive' && f.stateReason" class="mt-2 text-sm text-fog-300">Reason: {{ f.stateReason }}</p>
        </div>

        <dl class="mt-6 grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 border-t border-ink-700 pt-4 text-sm">
          <dt class="text-fog-400">Asset</dt>
          <dd class="min-w-0">
            <RouterLink :to="`/assets/${f.assetId}`" class="link inline-flex items-center gap-1.5 break-all" @click="emit('close')">
              <AppIcon :name="assetKindIcon[f.assetKind]" :size="13" />{{ f.assetName }}
            </RouterLink>
          </dd>
          <dt class="text-fog-400">Project</dt>
          <dd>{{ f.projectName ?? '—' }}</dd>
          <dt class="text-fog-400">Kind</dt>
          <dd>{{ kindLabel[f.kind] }}</dd>
          <template v-if="f.pkgName">
            <dt class="text-fog-400">Package</dt>
            <dd class="font-mono text-[0.8rem] break-all">
              {{ f.pkgName }}<span v-if="f.pkgEcosystem" class="text-fog-500"> ({{ f.pkgEcosystem }})</span>
            </dd>
            <dt class="text-fog-400">Version</dt>
            <dd class="font-mono text-[0.8rem]">
              <span class="text-fog-200">{{ f.installedVersion ?? '?' }}</span>
              <template v-if="f.fixedVersion">
                <span class="mx-1.5 text-fog-500" aria-label="fixed in">→</span><span class="text-ok">{{ f.fixedVersion }}</span>
              </template>
              <span v-else class="ml-2 font-sans text-xs text-fog-500">no fix available</span>
            </dd>
          </template>
          <dt class="text-fog-400">Location</dt>
          <dd class="font-mono text-[0.8rem] break-all text-fog-200">{{ f.target || '—' }}</dd>
          <dt class="text-fog-400">Reported by</dt>
          <dd>{{ f.sources.length ? f.sources.join(', ') : '—' }}</dd>
          <dt class="text-fog-400">First seen</dt>
          <dd class="num">{{ dateTime(f.firstSeen) }} <span class="text-fog-500">({{ ago(f.firstSeen) }})</span></dd>
          <dt class="text-fog-400">Last seen</dt>
          <dd class="num">{{ dateTime(f.lastSeen) }} <span class="text-fog-500">({{ ago(f.lastSeen) }})</span></dd>
          <template v-if="f.publishedAt">
            <dt class="text-fog-400">Published</dt>
            <dd class="num">{{ date(f.publishedAt) }}</dd>
          </template>
        </dl>

        <section v-if="f.description" class="mt-6 border-t border-ink-700 pt-4">
          <h3 class="eyebrow mb-2">Description</h3>
          <p class="text-sm leading-relaxed whitespace-pre-line text-fog-200" :class="longDesc && !descOpen ? 'line-clamp-[10]' : ''">{{ f.description }}</p>
          <button v-if="longDesc" type="button" class="link mt-1 text-xs" @click="descOpen = !descOpen">{{ descOpen ? 'Show less' : 'Show more' }}</button>
        </section>

        <section v-if="f.references?.length" class="mt-6 border-t border-ink-700 pt-4">
          <h3 class="eyebrow mb-2">References</h3>
          <ul class="space-y-1 text-sm">
            <li v-for="r in f.references" :key="r" class="min-w-0">
              <a v-if="isLink(r)" :href="r" target="_blank" rel="noopener noreferrer" class="link inline-flex max-w-full items-center gap-1.5">
                <AppIcon name="external-link" :size="12" class="shrink-0" />
                <span class="truncate">{{ host(r) }}<span class="text-fog-500">{{ r.replace(/^https?:\/\/[^/]+/i, '') }}</span></span>
              </a>
              <span v-else class="font-mono text-xs break-all text-fog-300">{{ r }}</span>
            </li>
          </ul>
        </section>

        <section class="mt-6 border-t border-ink-700 pt-4">
          <h3 class="eyebrow mb-3">History</h3>
          <p v-if="!events.length" class="text-sm text-fog-500">No state changes yet.</p>
          <ol v-else class="relative ml-1.5 space-y-3 border-l border-ink-600 pl-4">
            <li v-for="(ev, i) in events" :key="i" class="relative text-sm">
              <span class="absolute top-1.5 -left-[1.29rem] h-2 w-2 rounded-full border border-ink-850 bg-fog-400" :class="i === 0 ? '!bg-beam' : ''" />
              <p class="text-fog-100">
                <template v-if="ev.fromState">{{ stateLabel[ev.fromState] }} <span class="text-fog-500">→</span> </template>{{ stateLabel[ev.toState] }}
                <span class="text-fog-400">by {{ ev.actor }}</span>
              </p>
              <p class="num text-xs text-fog-500">{{ dateTime(ev.at) }}</p>
              <p v-if="ev.note" class="mt-0.5 text-xs text-fog-300">{{ ev.note }}</p>
            </li>
          </ol>
        </section>
      </template>
    </div>
  </SideDrawer>
</template>
