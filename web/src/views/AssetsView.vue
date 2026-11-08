<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import AppIcon from '../components/AppIcon.vue'
import PageHeader from '../components/PageHeader.vue'
import EmptyState from '../components/EmptyState.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import ConfirmInline from '../components/ConfirmInline.vue'
import AssetFormDialog from '../components/AssetFormDialog.vue'
import CoverageCells from '../components/CoverageCells.vue'
import OpenCounts from '../components/OpenCounts.vue'
import { api, errorMessage } from '../lib/api'
import { useAsync } from '../lib/useAsync'
import { ago, dateTime, plural } from '../lib/format'
import { ASSET_KINDS, assetKindIcon, assetKindLabel } from '../lib/labels'
import type { Asset, AssetKind } from '../lib/types'
import { useProjects } from '../stores/projects'
import { useToasts } from '../stores/toasts'

const route = useRoute()
const router = useRouter()
const projects = useProjects()
const toasts = useToasts()

const project = ref<number | null>(Number(route.query.project) || null)
const kind = ref<AssetKind | ''>((ASSET_KINDS as string[]).includes(String(route.query.kind)) ? (route.query.kind as AssetKind) : '')
const showRetired = ref(route.query.retired === 'true')
const q = ref('')

const assets = useAsync(() =>
  api.get<Asset[]>('/api/v1/assets', {
    project: project.value ?? undefined,
    kind: kind.value || undefined,
    retired: showRetired.value ? 'true' : undefined,
  }),
)

onMounted(() => {
  projects.ensure()
  assets.run()
})
watch([project, kind, showRetired], () => {
  const query: Record<string, string> = {}
  if (project.value) query.project = String(project.value)
  if (kind.value) query.kind = kind.value
  if (showRetired.value) query.retired = 'true'
  router.replace({ query })
  assets.run()
})

const rows = computed(() => {
  const needle = q.value.trim().toLowerCase()
  const list = assets.data.value ?? []
  const filtered = needle
    ? list.filter((a) => [a.name, a.identifier, a.projectName ?? '', ...a.tags].some((s) => s.toLowerCase().includes(needle)))
    : list
  // Retired last; then most urgent open findings; then name.
  return [...filtered].sort(
    (a, b) =>
      Number(!!a.retiredAt) - Number(!!b.retiredAt) ||
      b.open.p1 - a.open.p1 ||
      b.open.p2 - a.open.p2 ||
      b.open.total - a.open.total ||
      a.name.localeCompare(b.name),
  )
})
const isFiltered = computed(() => !!project.value || !!kind.value || !!q.value.trim())
const uncovered = computed(() => (assets.data.value ?? []).filter((a) => !a.retiredAt && !a.covered).length)

// --- create / edit ------------------------------------------------------------
const formOpen = ref(false)
const editing = ref<Asset | null>(null)
function openNew() {
  editing.value = null
  formOpen.value = true
}
function openEdit(a: Asset) {
  editing.value = a
  formOpen.value = true
}
function onSaved() {
  assets.run()
  projects.refresh()
}

// --- retire / delete ------------------------------------------------------------
const busyId = ref<number | null>(null)
async function setRetired(a: Asset, retired: boolean) {
  busyId.value = a.id
  try {
    await api.patch<Asset>(`/api/v1/assets/${a.id}`, { retired })
    toasts.ok(retired ? `Retired ${a.name}. It no longer counts toward coverage.` : `${a.name} is active again.`)
    await assets.run()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}
async function remove(a: Asset) {
  busyId.value = a.id
  try {
    await api.del(`/api/v1/assets/${a.id}`)
    toasts.ok(`Deleted ${a.name} with its findings and scans.`)
    await assets.run()
    projects.refresh()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}
</script>

<template>
  <div>
    <PageHeader title="Assets" eyebrow="Inventory">
      Everything you expect to be scanned. Assets are created by hand or discovered on first ingest; gaps show where scans are missing or stale.
      <template #actions>
        <button type="button" class="btn btn-primary" data-testid="asset-new" @click="openNew"><AppIcon name="plus" :size="14" />New asset</button>
      </template>
    </PageHeader>

    <div class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-2.5">
      <div class="relative w-full sm:w-64">
        <label for="asset-search" class="sr-only">Search assets</label>
        <AppIcon name="search" :size="14" class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-fog-500" />
        <input id="asset-search" v-model="q" type="search" class="input !pl-8" placeholder="Name, identifier or tag" />
      </div>
      <div class="select-wrap">
        <label for="asset-filter-project" class="sr-only">Project</label>
        <select id="asset-filter-project" v-model="project" class="select w-auto">
          <option :value="null">All projects</option>
          <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </div>
      <div class="select-wrap">
        <label for="asset-filter-kind" class="sr-only">Kind</label>
        <select id="asset-filter-kind" v-model="kind" class="select w-auto">
          <option value="">All kinds</option>
          <option v-for="k in ASSET_KINDS" :key="k" :value="k">{{ assetKindLabel[k] }}</option>
        </select>
      </div>
      <label class="inline-flex cursor-pointer items-center gap-2 text-sm text-fog-300">
        <input v-model="showRetired" type="checkbox" class="check" data-testid="show-retired" />Show retired
      </label>
      <p v-if="assets.data.value?.length" class="ml-auto text-xs text-fog-400">
        {{ plural(rows.length, 'asset') }}<template v-if="uncovered">, <span class="text-warn">{{ uncovered }} with gaps</span></template>
      </p>
    </div>

    <LoadingBlock v-if="assets.loading.value && !assets.data.value" :rows="6" label="Loading assets" />
    <ErrorBlock v-else-if="assets.error.value" :message="assets.error.value" @retry="assets.run()" />
    <template v-else-if="assets.data.value">
      <EmptyState v-if="!rows.length && isFiltered" icon="search" title="No matching assets" compact>Try a different search or filter.</EmptyState>
      <EmptyState v-else-if="!rows.length" icon="server" title="No assets yet">
        Add the repositories, images, hosts and clusters you expect to be scanned, or push a scan and the asset is discovered automatically.
        <template #actions>
          <button type="button" class="btn btn-primary" @click="openNew"><AppIcon name="plus" :size="14" />Add your first asset</button>
          <RouterLink to="/sources" class="btn">Create a source</RouterLink>
        </template>
      </EmptyState>

      <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
        <table class="dtable min-w-[64rem]">
          <thead>
            <tr>
              <th>Asset</th>
              <th>Project</th>
              <th>Tags</th>
              <th>Open</th>
              <th>Coverage</th>
              <th class="text-right">Last scan</th>
              <th class="w-px"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="a in rows" :key="a.id" data-testid="asset-row" :data-identifier="a.identifier" :data-id="a.id" :class="a.retiredAt ? 'opacity-55' : ''">
              <td class="max-w-[22rem]">
                <div class="flex items-center gap-2.5">
                  <span class="grid h-7 w-7 shrink-0 place-items-center rounded-[4px] border border-ink-600 bg-ink-850 text-fog-300" :title="assetKindLabel[a.kind]">
                    <AppIcon :name="assetKindIcon[a.kind]" :size="14" :label="assetKindLabel[a.kind]" />
                  </span>
                  <div class="min-w-0">
                    <div class="flex items-center gap-1.5">
                      <RouterLink :to="`/assets/${a.id}`" class="truncate font-medium text-fog-100 hover:text-beam">{{ a.name }}</RouterLink>
                      <span v-if="a.discovered" class="tag !h-4 !text-[0.6rem] tag-accent" title="Created by an ingest, not by hand" data-testid="asset-discovered">discovered</span>
                      <span v-if="a.retiredAt" class="tag !h-4 !text-[0.6rem]">retired</span>
                    </div>
                    <p v-if="a.identifier !== a.name" class="truncate font-mono text-[0.7rem] text-fog-500" :title="a.identifier">{{ a.identifier }}</p>
                  </div>
                </div>
              </td>
              <td class="text-fog-300">{{ a.projectName ?? '—' }}</td>
              <td class="max-w-[12rem]">
                <span class="flex flex-wrap gap-1">
                  <span v-for="t in a.tags" :key="t" class="tag" :class="t === 'internet-facing' ? 'tag-accent' : ''">{{ t }}</span>
                  <span v-if="!a.tags.length" class="text-fog-500">—</span>
                </span>
              </td>
              <td>
                <RouterLink v-if="a.open.total" :to="{ path: '/findings', query: { asset: String(a.id) } }" class="hover:underline">
                  <OpenCounts :open="a.open" />
                </RouterLink>
                <OpenCounts v-else :open="a.open" />
              </td>
              <td><CoverageCells :cells="a.coverage" /></td>
              <td class="num text-right text-xs whitespace-nowrap text-fog-400" :title="dateTime(a.lastScanAt)">{{ ago(a.lastScanAt) }}</td>
              <td>
                <div class="flex items-center justify-end gap-1">
                  <ConfirmInline :question="`Delete ${a.name} and all its findings?`" :busy="busyId === a.id" testid="asset-delete-confirm" @confirm="remove(a)">
                    <template #default="{ ask }">
                      <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Edit ${a.name}`" data-testid="asset-edit" @click="openEdit(a)">
                        <AppIcon name="pencil" :size="13" />
                      </button>
                      <button
                        v-if="!a.retiredAt"
                        type="button"
                        class="btn btn-ghost btn-sm"
                        :disabled="busyId === a.id"
                        :aria-label="`Retire ${a.name}`"
                        title="Retire: keep history, stop expecting scans"
                        data-testid="asset-retire"
                        @click="setRetired(a, true)"
                      >
                        <AppIcon name="archive" :size="13" />
                      </button>
                      <button
                        v-else
                        type="button"
                        class="btn btn-ghost btn-sm"
                        :disabled="busyId === a.id"
                        :aria-label="`Unretire ${a.name}`"
                        title="Unretire"
                        data-testid="asset-unretire"
                        @click="setRetired(a, false)"
                      >
                        <AppIcon name="archive-restore" :size="13" />
                      </button>
                      <button type="button" class="btn btn-ghost btn-sm hover:!text-[#ff8a99]" :aria-label="`Delete ${a.name}`" data-testid="asset-delete" @click="ask">
                        <AppIcon name="trash-2" :size="13" />
                      </button>
                    </template>
                  </ConfirmInline>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>

    <AssetFormDialog :open="formOpen" :asset="editing" @close="formOpen = false" @saved="onSaved" />
  </div>
</template>
