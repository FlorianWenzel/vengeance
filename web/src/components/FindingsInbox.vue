<script setup lang="ts">
// The findings inbox: filters, table, pagination, bulk triage and the detail drawer.
// Used on /findings (filters synced to the URL) and on the asset detail page (locked to one asset).
import {
  computed,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch,
} from "vue";
import { useRoute, useRouter, type LocationQuery } from "vue-router";
import AppIcon from "./AppIcon.vue";
import PriorityChip from "./PriorityChip.vue";
import SeverityText from "./SeverityText.vue";
import StateBadge from "./StateBadge.vue";
import EmptyState from "./EmptyState.vue";
import LoadingBlock from "./LoadingBlock.vue";
import ErrorBlock from "./ErrorBlock.vue";
import TriageControls from "./TriageControls.vue";
import FindingDrawer from "./FindingDrawer.vue";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ago, dateTime, num } from "../lib/format";
import {
  KINDS,
  PRIORITIES,
  assetKindIcon,
  kindLabel,
  stateFilterOptions,
} from "../lib/labels";
import type {
  Asset,
  FindingList,
  Kind,
  Priority,
  StateFilter,
} from "../lib/types";
import { useProjects } from "../stores/projects";

const props = withDefaults(
  defineProps<{ assetId?: number; syncUrl?: boolean }>(),
  { syncUrl: false },
);

const route = useRoute();
const router = useRouter();
const projects = useProjects();

type Sort = "priority" | "lastSeen" | "firstSeen";
interface Filters {
  state: StateFilter;
  priority: Priority[];
  kind: Kind | "";
  asset: number | null;
  project: number | null;
  q: string;
  sort: Sort;
  limit: number;
  offset: number;
}

const DEFAULTS: Filters = {
  state: "open",
  priority: [],
  kind: "",
  asset: null,
  project: null,
  q: "",
  sort: "priority",
  limit: 50,
  offset: 0,
};
const LIMITS = [25, 50, 100, 200];
const STATES = new Set(stateFilterOptions.map((o) => o.value));

function str(v: LocationQuery[string]): string {
  return (Array.isArray(v) ? v[0] : v) ?? "";
}
function intOrNull(v: string): number | null {
  const n = Number(v);
  return v !== "" && Number.isInteger(n) && n > 0 ? n : null;
}

function fromQuery(q: LocationQuery): Filters {
  const state = str(q.state);
  const sort = str(q.sort);
  const limit = Number(str(q.limit));
  const offset = Number(str(q.offset));
  const kind = str(q.kind);
  return {
    state: (STATES.has(state) ? state : "open") as StateFilter,
    priority: str(q.priority)
      .split(",")
      .map(Number)
      .filter((p): p is Priority => p >= 1 && p <= 4)
      .sort(),
    kind: (KINDS as string[]).includes(kind) ? (kind as Kind) : "",
    asset: intOrNull(str(q.asset)),
    project: intOrNull(str(q.project)),
    q: str(q.q),
    sort: sort === "lastSeen" || sort === "firstSeen" ? sort : "priority",
    limit: LIMITS.includes(limit) ? limit : 50,
    offset: Number.isInteger(offset) && offset > 0 ? offset : 0,
  };
}

function toQuery(f: Filters, drawer: number | null): Record<string, string> {
  const q: Record<string, string> = {};
  if (f.state !== "open") q.state = f.state;
  if (f.priority.length) q.priority = f.priority.join(",");
  if (f.kind) q.kind = f.kind;
  if (f.asset) q.asset = String(f.asset);
  if (f.project) q.project = String(f.project);
  if (f.q) q.q = f.q;
  if (f.sort !== "priority") q.sort = f.sort;
  if (f.limit !== 50) q.limit = String(f.limit);
  if (f.offset) q.offset = String(f.offset);
  if (drawer) q.finding = String(drawer);
  return q;
}

const filters = reactive<Filters>(
  props.syncUrl ? fromQuery(route.query) : { ...DEFAULTS },
);
const drawerId = ref<number | null>(
  props.syncUrl ? intOrNull(str(route.query.finding)) : null,
);
const qInput = ref(filters.q);

const effectiveAsset = computed(() => props.assetId ?? filters.asset);

const list = useAsync(() =>
  api.get<FindingList>("/api/v1/findings", {
    state: filters.state,
    priority: filters.priority.join(",") || undefined,
    kind: filters.kind || undefined,
    asset: effectiveAsset.value ?? undefined,
    project: filters.project ?? undefined,
    q: filters.q || undefined,
    sort: filters.sort,
    limit: filters.limit,
    offset: filters.offset,
  }),
);

// When the default inbox is empty, find out whether anything was ever ingested,
// so the empty state can say "inbox zero" vs "no findings yet".
const everAny = ref<boolean | null>(null);
async function probeEver() {
  try {
    const r = await api.get<FindingList>("/api/v1/findings", {
      state: "all",
      limit: 1,
      asset: props.assetId ?? undefined,
    });
    everAny.value = (r?.total ?? 0) > 0;
  } catch {
    everAny.value = null;
  }
}

const isFiltered = computed(
  () =>
    filters.state !== "open" ||
    filters.priority.length > 0 ||
    !!filters.kind ||
    !!filters.asset ||
    !!filters.project ||
    !!filters.q,
);

async function load() {
  await list.run();
  const d = list.data.value;
  // Paged past the end (e.g. after triaging the last rows of a page): step back.
  if (d && d.items.length === 0 && d.total > 0 && filters.offset > 0) {
    filters.offset = Math.max(
      0,
      Math.floor((d.total - 1) / filters.limit) * filters.limit,
    );
    return;
  }
  if (d && d.total === 0 && !isFiltered.value) probeEver();
}

// Assets for the dropdown (only when not locked to one asset).
const assets = useAsync(() => api.get<Asset[]>("/api/v1/assets"));

onMounted(() => {
  projects.ensure();
  if (props.assetId === undefined) assets.run();
});

// --- filter <-> url ---------------------------------------------------------
let syncingFromUrl = false;
const filterKey = computed(() => JSON.stringify(filters));

watch(filterKey, (now, before) => {
  const a = JSON.parse(now) as Filters;
  const b = JSON.parse(before) as Filters;
  // Any change other than paging resets to the first page.
  if (a.offset === b.offset && a.offset !== 0 && !syncingFromUrl) {
    filters.offset = 0;
    return;
  }
  selected.value = new Set();
  if (props.syncUrl && !syncingFromUrl) {
    router.replace({ query: toQuery(filters, drawerId.value) });
  }
  load();
});

if (props.syncUrl) {
  watch(
    () => route.query,
    (q) => {
      if (route.path !== "/findings") return;
      const next = fromQuery(q);
      const did = intOrNull(str(q.finding));
      if (did !== drawerId.value) drawerId.value = did;
      if (JSON.stringify(next) !== filterKey.value) {
        syncingFromUrl = true;
        Object.assign(filters, next);
        qInput.value = next.q;
        // let the filterKey watcher run with the flag set
        Promise.resolve().then(() => (syncingFromUrl = false));
      }
    },
  );
  watch(drawerId, (id) => {
    const want = id ? String(id) : undefined;
    if (str(route.query.finding) !== (want ?? ""))
      router.replace({ query: toQuery(filters, id) });
  });
}

watch(
  () => props.assetId,
  () => load(),
);

let qTimer = 0;
watch(qInput, (v) => {
  window.clearTimeout(qTimer);
  qTimer = window.setTimeout(() => (filters.q = v.trim()), 300);
});
onBeforeUnmount(() => window.clearTimeout(qTimer));

load();

function togglePriority(p: Priority) {
  const s = new Set(filters.priority);
  if (s.has(p)) s.delete(p);
  else s.add(p);
  filters.priority = [...s].sort() as Priority[];
}

function clearFilters() {
  qInput.value = "";
  Object.assign(filters, { ...DEFAULTS, limit: filters.limit });
}

// --- selection ----------------------------------------------------------------
const selected = ref<Set<number>>(new Set());
const items = computed(() => list.data.value?.items ?? []);
const total = computed(() => list.data.value?.total ?? 0);
const allOnPage = computed(
  () =>
    items.value.length > 0 &&
    items.value.every((f) => selected.value.has(f.id)),
);
const someOnPage = computed(() =>
  items.value.some((f) => selected.value.has(f.id)),
);

function toggle(id: number) {
  const s = new Set(selected.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  selected.value = s;
}
function toggleAll() {
  selected.value = allOnPage.value
    ? new Set()
    : new Set(items.value.map((f) => f.id));
}

function onTriaged() {
  selected.value = new Set();
  load();
}

// --- paging -------------------------------------------------------------------
const pageFrom = computed(() => (total.value ? filters.offset + 1 : 0));
const pageTo = computed(() =>
  Math.min(total.value, filters.offset + items.value.length),
);
function prev() {
  filters.offset = Math.max(0, filters.offset - filters.limit);
}
function next() {
  if (filters.offset + filters.limit < total.value)
    filters.offset += filters.limit;
}

function openRow(id: number) {
  drawerId.value = id;
}
function onRowKey(e: KeyboardEvent, id: number) {
  if (e.target !== e.currentTarget) return;
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    openRow(id);
  }
}

const showAssetCol = computed(() => props.assetId === undefined);
const assetOptions = computed(() =>
  [...(assets.data.value ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
);
</script>

<template>
  <div :class="selected.size ? 'pb-20' : ''">
    <!-- Filters -->
    <div class="mb-3 flex flex-wrap items-end gap-x-3 gap-y-2.5">
      <div class="relative w-full sm:w-72">
        <label for="findings-search" class="sr-only">Search findings</label>
        <AppIcon
          name="search"
          :size="14"
          class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-fog-500"
        />
        <input
          id="findings-search"
          v-model="qInput"
          type="search"
          class="input !pl-8"
          placeholder="CVE, title or package"
          autocomplete="off"
          data-testid="findings-search"
        />
      </div>

      <div>
        <label for="filter-state" class="sr-only">State</label>
        <div class="select-wrap">
          <select
            id="filter-state"
            v-model="filters.state"
            class="select w-auto"
            data-testid="filter-state"
          >
            <option
              v-for="o in stateFilterOptions"
              :key="o.value"
              :value="o.value"
            >
              {{ o.label }}
            </option>
          </select>
        </div>
      </div>

      <fieldset class="flex items-center gap-1">
        <legend class="sr-only">Priority</legend>
        <button
          v-for="p in PRIORITIES"
          :key="p"
          type="button"
          class="pfilter"
          :class="filters.priority.includes(p) ? 'is-on' : ''"
          :style="{ '--pc': `var(--color-p${p})` }"
          :aria-pressed="filters.priority.includes(p)"
          :data-testid="`filter-p${p}`"
          @click="togglePriority(p)"
        >
          P{{ p }}
        </button>
      </fieldset>

      <div>
        <label for="filter-kind" class="sr-only">Kind</label>
        <div class="select-wrap">
          <select
            id="filter-kind"
            v-model="filters.kind"
            class="select w-auto"
            data-testid="filter-kind"
          >
            <option value="">All kinds</option>
            <option v-for="k in KINDS" :key="k" :value="k">
              {{ kindLabel[k] }}
            </option>
          </select>
        </div>
      </div>

      <div v-if="showAssetCol">
        <label for="filter-asset" class="sr-only">Asset</label>
        <div class="select-wrap">
          <select
            id="filter-asset"
            v-model="filters.asset"
            class="select w-auto max-w-[14rem]"
            data-testid="filter-asset"
          >
            <option :value="null">All assets</option>
            <option v-for="a in assetOptions" :key="a.id" :value="a.id">
              {{ a.name }}
            </option>
          </select>
        </div>
      </div>

      <div v-if="showAssetCol">
        <label for="filter-project" class="sr-only">Project</label>
        <div class="select-wrap">
          <select
            id="filter-project"
            v-model="filters.project"
            class="select w-auto max-w-[12rem]"
            data-testid="filter-project"
          >
            <option :value="null">All projects</option>
            <option v-for="p in projects.list" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select>
        </div>
      </div>

      <div class="ml-auto flex items-center gap-2">
        <label for="filter-sort" class="text-xs text-fog-400">Sort</label>
        <div class="select-wrap">
          <select
            id="filter-sort"
            v-model="filters.sort"
            class="select w-auto"
            data-testid="filter-sort"
          >
            <option value="priority">Priority</option>
            <option value="lastSeen">Last seen</option>
            <option value="firstSeen">First seen</option>
          </select>
        </div>
      </div>
    </div>

    <div
      class="mb-2 flex min-h-6 items-center justify-between gap-3 text-xs text-fog-400"
    >
      <p aria-live="polite">
        <span
          class="num text-sm font-semibold text-fog-100"
          data-testid="findings-total"
          >{{ list.data.value ? total : "…" }}</span
        >
        {{ total === 1 ? "finding" : "findings" }}
        <span
          v-if="list.loading.value && list.data.value"
          class="ml-2 inline-flex items-center gap-1 text-fog-500"
        >
          <AppIcon
            name="loader-circle"
            :size="12"
            class="animate-spin"
          />updating
        </span>
      </p>
      <button
        v-if="isFiltered"
        type="button"
        class="link"
        @click="clearFilters"
      >
        Clear filters
      </button>
    </div>

    <LoadingBlock
      v-if="list.loading.value && !list.data.value"
      :rows="8"
      label="Loading findings"
    />
    <ErrorBlock
      v-else-if="list.error.value"
      :message="list.error.value"
      @retry="load"
    />

    <template v-else-if="list.data.value">
      <template v-if="!items.length">
        <EmptyState
          v-if="isFiltered"
          icon="search"
          title="Nothing matches"
          compact
        >
          No findings match these filters.
          <template #actions
            ><button type="button" class="btn" @click="clearFilters">
              Clear filters
            </button></template
          >
        </EmptyState>
        <EmptyState v-else-if="everAny" icon="circle-check" title="Inbox zero">
          Every finding is fixed, accepted or dismissed. New ones land here as
          soon as a scan reports them.
          <template #actions>
            <button type="button" class="btn" @click="filters.state = 'all'">
              Show all states
            </button>
          </template>
        </EmptyState>
        <EmptyState
          v-else-if="assetId !== undefined"
          icon="inbox"
          title="No findings on this asset"
        >
          Push a scan for this asset from CI with a source token, and its
          findings appear here.
          <template #actions
            ><RouterLink to="/sources" class="btn btn-primary"
              >Set up a source</RouterLink
            ></template
          >
        </EmptyState>
        <EmptyState v-else icon="inbox" title="No findings yet">
          Create a source and push a Trivy report. Results are deduplicated into
          findings, prioritised P1 to P4, and land in this inbox.
          <template #actions
            ><RouterLink to="/sources" class="btn btn-primary"
              ><AppIcon name="radio-tower" :size="14" />Create a
              source</RouterLink
            ></template
          >
        </EmptyState>
      </template>

      <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
        <table
          class="dtable min-w-[60rem]"
          :class="list.loading.value ? 'opacity-60' : ''"
        >
          <thead>
            <tr>
              <th class="w-8 !pr-0">
                <input
                  type="checkbox"
                  class="check"
                  :checked="allOnPage"
                  :indeterminate="someOnPage && !allOnPage"
                  aria-label="Select all on this page"
                  data-testid="finding-select-all"
                  @change="toggleAll"
                />
              </th>
              <th class="w-14">Prio</th>
              <th class="w-24">Severity</th>
              <th>Advisory</th>
              <th>Package</th>
              <th v-if="showAssetCol">Asset</th>
              <th class="hidden xl:table-cell">Sources</th>
              <th v-if="filters.state !== 'open'">State</th>
              <th class="w-24 text-right">Last seen</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="f in items"
              :key="f.id"
              data-testid="finding-row"
              :data-id="f.id"
              tabindex="0"
              class="cursor-pointer"
              :class="[
                selected.has(f.id) ? 'is-selected' : '',
                drawerId === f.id ? 'is-active' : '',
              ]"
              :aria-label="`${f.advisoryId}: ${f.title}`"
              @click="openRow(f.id)"
              @keydown="onRowKey($event, f.id)"
            >
              <td class="!pr-0" @click.stop>
                <input
                  type="checkbox"
                  class="check"
                  :checked="selected.has(f.id)"
                  :aria-label="`Select ${f.advisoryId}`"
                  data-testid="finding-select"
                  @change="toggle(f.id)"
                />
              </td>
              <td><PriorityChip :p="f.priority" /></td>
              <td><SeverityText :severity="f.severity" /></td>
              <td class="max-w-[28rem]">
                <div class="flex items-center gap-1.5">
                  <span
                    class="font-mono text-[0.8rem] font-medium whitespace-nowrap text-fog-100"
                    >{{ f.advisoryId }}</span
                  >
                  <span
                    v-if="f.kev"
                    class="inline-flex items-center gap-0.5 rounded-[3px] border border-p1/60 px-1 font-mono text-[0.62rem] font-bold text-[#ff8a99]"
                    title="Known exploited (CISA KEV)"
                  >
                    <AppIcon name="flame" :size="10" />KEV
                  </span>
                  <span
                    v-if="f.kind !== 'vuln'"
                    class="tag !h-4 !text-[0.62rem]"
                    >{{ f.kind }}</span
                  >
                </div>
                <p class="truncate text-xs text-fog-400" :title="f.title">
                  {{ f.title }}
                </p>
              </td>
              <td class="font-mono text-[0.78rem] whitespace-nowrap">
                <template v-if="f.pkgName">
                  <span class="text-fog-100">{{ f.pkgName }}</span>
                  <span class="ml-1.5 text-fog-400">{{
                    f.installedVersion ?? "?"
                  }}</span>
                  <template v-if="f.fixedVersion">
                    <span class="mx-1 text-fog-500">→</span
                    ><span class="text-ok">{{ f.fixedVersion }}</span>
                  </template>
                  <span
                    v-else
                    class="ml-1.5 font-sans text-[0.7rem] text-fog-500"
                    >no fix</span
                  >
                </template>
                <span
                  v-else
                  class="block max-w-[16rem] truncate text-fog-400"
                  :title="f.target"
                  >{{ f.target || "—" }}</span
                >
              </td>
              <td v-if="showAssetCol" class="max-w-[14rem]">
                <span class="flex items-center gap-1.5 text-fog-200">
                  <AppIcon
                    :name="assetKindIcon[f.assetKind]"
                    :size="13"
                    class="text-fog-500"
                  />
                  <span class="truncate" :title="f.assetName">{{
                    f.assetName
                  }}</span>
                </span>
                <span
                  v-if="f.projectName"
                  class="block truncate pl-5 text-[0.7rem] text-fog-500"
                  >{{ f.projectName }}</span
                >
              </td>
              <td
                class="hidden max-w-[10rem] truncate text-xs text-fog-400 xl:table-cell"
                :title="f.sources.join(', ')"
              >
                {{ f.sources.join(", ") || "—" }}
              </td>
              <td v-if="filters.state !== 'open'">
                <StateBadge :state="f.state" />
              </td>
              <td
                class="num text-right text-xs whitespace-nowrap text-fog-400"
                :title="dateTime(f.lastSeen)"
              >
                {{ ago(f.lastSeen) }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div
        v-if="total > 0"
        class="mt-3 flex flex-wrap items-center justify-between gap-3 text-xs text-fog-400"
      >
        <p class="num">
          {{ num(pageFrom) }}–{{ num(pageTo) }} of {{ num(total) }}
        </p>
        <div class="flex items-center gap-2">
          <label for="page-size" class="sr-only">Rows per page</label>
          <div class="select-wrap">
            <select
              id="page-size"
              v-model.number="filters.limit"
              class="select !h-7 w-auto !text-xs"
            >
              <option v-for="l in LIMITS" :key="l" :value="l">
                {{ l }} / page
              </option>
            </select>
          </div>
          <button
            type="button"
            class="btn btn-sm"
            :disabled="filters.offset === 0"
            aria-label="Previous page"
            data-testid="page-prev"
            @click="prev"
          >
            <AppIcon name="chevron-left" :size="14" />
          </button>
          <button
            type="button"
            class="btn btn-sm"
            :disabled="filters.offset + filters.limit >= total"
            aria-label="Next page"
            data-testid="page-next"
            @click="next"
          >
            <AppIcon name="chevron-right" :size="14" />
          </button>
        </div>
      </div>
    </template>

    <!-- Bulk action bar -->
    <Transition name="bulk">
      <div
        v-if="selected.size"
        class="fixed inset-x-0 bottom-4 z-30 mx-auto flex w-[min(52rem,calc(100vw-2rem))] flex-wrap items-center gap-3 rounded-md border border-beam/40 bg-ink-800/95 px-4 py-2.5 shadow-[0_20px_60px_-15px_rgba(0,0,0,.9)] backdrop-blur lg:left-60"
        role="region"
        aria-label="Bulk actions"
        data-testid="bulk-bar"
      >
        <p class="text-sm text-fog-100">
          <span class="num font-semibold text-beam">{{ selected.size }}</span>
          selected
        </p>
        <TriageControls
          :ids="[...selected]"
          test-prefix="bulk"
          @done="onTriaged"
        />
        <button
          type="button"
          class="btn btn-ghost btn-sm ml-auto"
          @click="selected = new Set()"
        >
          Clear
        </button>
      </div>
    </Transition>

    <FindingDrawer :id="drawerId" @close="drawerId = null" @changed="load" />
  </div>
</template>

<style scoped>
.pfilter {
  height: 2rem;
  min-width: 2.4rem;
  padding: 0 0.55rem;
  border-radius: 4px;
  border: 1px solid var(--color-ink-600);
  background: var(--color-ink-950);
  color: var(--color-fog-400);
  font-family: var(--font-mono);
  font-size: 0.75rem;
  font-weight: 700;
  cursor: pointer;
}
.pfilter:hover {
  color: var(--pc);
  border-color: color-mix(in oklab, var(--pc) 50%, var(--color-ink-600));
}
.pfilter.is-on {
  color: var(--pc);
  border-color: var(--pc);
  background: color-mix(in oklab, var(--pc) 14%, transparent);
}
.bulk-enter-active,
.bulk-leave-active {
  transition:
    opacity 150ms,
    transform 150ms;
}
.bulk-enter-from,
.bulk-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>
