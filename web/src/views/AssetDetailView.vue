<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import AppIcon from '../components/AppIcon.vue'
import EmptyState from '../components/EmptyState.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import ConfirmInline from '../components/ConfirmInline.vue'
import AssetFormDialog from '../components/AssetFormDialog.vue'
import CoverageCells from '../components/CoverageCells.vue'
import OpenCounts from '../components/OpenCounts.vue'
import FindingsInbox from '../components/FindingsInbox.vue'
import { api, errorMessage } from '../lib/api'
import { useAsync } from '../lib/useAsync'
import { ago, dateTime } from '../lib/format'
import { assetKindIcon, assetKindLabel, scanTypeLabel } from '../lib/labels'
import type { AssetDetail } from '../lib/types'
import { useToasts } from '../stores/toasts'

const props = defineProps<{ id: number }>()
const router = useRouter()
const toasts = useToasts()

const asset = useAsync(() => api.get<AssetDetail>(`/api/v1/assets/${props.id}`))
watch(
  () => props.id,
  () => asset.run(),
  { immediate: true },
)
const a = computed(() => asset.data.value)
const scans = computed(() => [...(a.value?.scans ?? [])].sort((x, y) => Date.parse(y.receivedAt) - Date.parse(x.receivedAt)))

const formOpen = ref(false)
const busy = ref(false)

async function setRetired(retired: boolean) {
  if (!a.value) return
  busy.value = true
  try {
    await api.patch(`/api/v1/assets/${a.value.id}`, { retired })
    toasts.ok(retired ? 'Asset retired.' : 'Asset is active again.')
    await asset.run()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busy.value = false
  }
}
async function remove() {
  if (!a.value) return
  busy.value = true
  try {
    await api.del(`/api/v1/assets/${a.value.id}`)
    toasts.ok(`Deleted ${a.value.name}.`)
    router.push('/assets')
  } catch (e) {
    toasts.error(errorMessage(e))
    busy.value = false
  }
}
</script>

<template>
  <div>
    <RouterLink to="/assets" class="mb-3 inline-flex items-center gap-1 text-xs text-fog-400 hover:text-fog-100">
      <AppIcon name="chevron-left" :size="13" />Assets
    </RouterLink>

    <LoadingBlock v-if="asset.loading.value && !a" :rows="5" label="Loading asset" />
    <ErrorBlock v-else-if="asset.error.value && !a" :message="asset.error.value" @retry="asset.run()" />
    <template v-else-if="a">
      <header class="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div class="flex min-w-0 items-start gap-3">
          <span class="mt-1 grid h-10 w-10 shrink-0 place-items-center rounded-md border border-ink-600 bg-ink-850 text-beam">
            <AppIcon :name="assetKindIcon[a.kind]" :size="19" :label="assetKindLabel[a.kind]" />
          </span>
          <div class="min-w-0">
            <p class="eyebrow">{{ assetKindLabel[a.kind] }}</p>
            <h1 class="text-[2rem] leading-tight break-all text-fog-100 normal-case">{{ a.name }}</h1>
            <p class="font-mono text-xs break-all text-fog-400">{{ a.kind }}:{{ a.identifier }}</p>
            <div class="mt-2 flex flex-wrap gap-1.5">
              <span v-if="a.discovered" class="tag tag-accent">discovered</span>
              <span v-if="a.retiredAt" class="tag">retired {{ ago(a.retiredAt) }}</span>
              <span v-for="t in a.tags" :key="t" class="tag" :class="t === 'internet-facing' ? 'tag-accent' : ''">{{ t }}</span>
            </div>
          </div>
        </div>
        <ConfirmInline :question="`Delete ${a.name}, its findings and scans?`" :busy="busy" testid="asset-delete-confirm" @confirm="remove">
          <template #default="{ ask }">
            <button type="button" class="btn" data-testid="asset-edit" @click="formOpen = true"><AppIcon name="pencil" :size="13" />Edit</button>
            <button v-if="!a.retiredAt" type="button" class="btn" :disabled="busy" data-testid="asset-retire" @click="setRetired(true)">
              <AppIcon name="archive" :size="13" />Retire
            </button>
            <button v-else type="button" class="btn" :disabled="busy" data-testid="asset-unretire" @click="setRetired(false)">
              <AppIcon name="archive-restore" :size="13" />Unretire
            </button>
            <button type="button" class="btn btn-danger" data-testid="asset-delete" @click="ask"><AppIcon name="trash-2" :size="13" />Delete</button>
          </template>
        </ConfirmInline>
      </header>

      <dl class="grid grid-cols-2 gap-x-8 gap-y-4 border-y border-ink-700 py-4 text-sm sm:grid-cols-3 lg:grid-cols-5">
        <div>
          <dt class="eyebrow !text-[0.7rem]">Project</dt>
          <dd class="mt-1 text-fog-100">{{ a.projectName ?? '—' }}</dd>
        </div>
        <div>
          <dt class="eyebrow !text-[0.7rem]">Open findings</dt>
          <dd class="mt-1"><OpenCounts :open="a.open" /></dd>
        </div>
        <div>
          <dt class="eyebrow !text-[0.7rem]">Last scan</dt>
          <dd class="num mt-1 text-fog-100" :title="dateTime(a.lastScanAt)">{{ ago(a.lastScanAt) }}</dd>
        </div>
        <div>
          <dt class="eyebrow !text-[0.7rem]">Added</dt>
          <dd class="num mt-1 text-fog-100" :title="dateTime(a.createdAt)">{{ ago(a.createdAt) }}</dd>
        </div>
        <div class="col-span-2 sm:col-span-3 lg:col-span-1">
          <dt class="eyebrow !text-[0.7rem]">Coverage</dt>
          <dd class="mt-1"><CoverageCells :cells="a.coverage" /></dd>
        </div>
      </dl>

      <section class="mt-8">
        <h2 class="mb-3 text-xl text-fog-100">Findings</h2>
        <FindingsInbox :asset-id="a.id" />
      </section>

      <section class="mt-10">
        <h2 class="mb-3 text-xl text-fog-100">Scan history</h2>
        <EmptyState v-if="!scans.length" icon="clock" title="No scans yet" compact>
          Push a report with <code class="font-mono text-xs text-fog-200">asset={{ a.kind }}:{{ a.identifier }}</code> and it shows up here.
          <template #actions><RouterLink to="/sources" class="btn">Sources and tokens</RouterLink></template>
        </EmptyState>
        <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
          <table class="dtable min-w-[40rem]">
            <thead>
              <tr>
                <th>Received</th>
                <th>Source</th>
                <th>Format</th>
                <th>Scan types</th>
                <th>Digest</th>
                <th class="text-right">Findings</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="s in scans" :key="s.id" data-testid="scan-row">
                <td class="num whitespace-nowrap" :title="dateTime(s.receivedAt)">{{ ago(s.receivedAt) }}</td>
                <td>{{ s.sourceName }}</td>
                <td><span class="tag">{{ s.format }}</span></td>
                <td class="text-xs text-fog-300">{{ s.scanTypes.map((t) => scanTypeLabel[t] ?? t).join(', ') || '—' }}</td>
                <td class="max-w-[14rem] truncate font-mono text-[0.7rem] text-fog-400" :title="s.digest ?? ''">{{ s.digest ?? '—' }}</td>
                <td class="num text-right">{{ s.findingCount }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </template>

    <AssetFormDialog :open="formOpen" :asset="a" @close="formOpen = false" @saved="asset.run()" />
  </div>
</template>
