<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import AppIcon from "../components/AppIcon.vue";
import PageHeader from "../components/PageHeader.vue";
import EmptyState from "../components/EmptyState.vue";
import LoadingBlock from "../components/LoadingBlock.vue";
import ErrorBlock from "../components/ErrorBlock.vue";
import ScoreMeter from "../components/ScoreMeter.vue";
import SearchBeam from "../components/SearchBeam.vue";
import { api, errorMessage } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ago, dateTime, pct, plural } from "../lib/format";
import {
  ASSET_KINDS,
  SCAN_TYPES,
  assetKindIcon,
  assetKindLabel,
  scanTypeLabel,
} from "../lib/labels";
import type { Coverage, Expectation } from "../lib/types";
import { useToasts } from "../stores/toasts";

const toasts = useToasts();
const cov = useAsync(() => api.get<Coverage>("/api/v1/coverage"));
const exp = useAsync(() => api.get<Expectation[]>("/api/v1/expectations"));

interface Row extends Expectation {
  key: number;
}
let seq = 0;
const rows = ref<Row[]>([]);
const saving = ref(false);
const saveError = ref<string | null>(null);

async function loadExpectations() {
  await exp.run();
  resetRows();
}
function resetRows() {
  rows.value = (exp.data.value ?? []).map((e) => ({ ...e, key: ++seq }));
  saveError.value = null;
}

onMounted(() => {
  cov.run();
  loadExpectations();
});

const sortedGaps = computed(() =>
  [...(cov.data.value?.gaps ?? [])].sort((a, b) => {
    // never-scanned first, then oldest
    const ta = a.lastAt ? Date.parse(a.lastAt) : -Infinity;
    const tb = b.lastAt ? Date.parse(b.lastAt) : -Infinity;
    return ta - tb || a.assetName.localeCompare(b.assetName);
  }),
);

const dupKeys = computed(() => {
  const seen = new Map<string, number>();
  for (const r of rows.value)
    seen.set(
      `${r.kind}/${r.scanType}`,
      (seen.get(`${r.kind}/${r.scanType}`) ?? 0) + 1,
    );
  return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
});
const invalid = computed(
  () =>
    dupKeys.value.size > 0 ||
    rows.value.some(
      (r) =>
        !Number.isInteger(r.maxAgeDays) ||
        r.maxAgeDays < 1 ||
        r.maxAgeDays > 3650,
    ),
);
const dirty = computed(() => {
  const strip = (l: Expectation[]) =>
    JSON.stringify(
      l.map(({ kind, scanType, maxAgeDays }) => ({
        kind,
        scanType,
        maxAgeDays,
      })),
    );
  return strip(rows.value) !== strip(exp.data.value ?? []);
});

function addRow() {
  // Suggest the first kind/scanType pair that is not already expected.
  const used = new Set(rows.value.map((r) => `${r.kind}/${r.scanType}`));
  let kind = ASSET_KINDS[0];
  let scanType = SCAN_TYPES[0];
  outer: for (const k of ASSET_KINDS) {
    for (const s of SCAN_TYPES) {
      if (!used.has(`${k}/${s}`)) {
        kind = k;
        scanType = s;
        break outer;
      }
    }
  }
  rows.value.push({ key: ++seq, kind, scanType, maxAgeDays: 7 });
}
function removeRow(key: number) {
  rows.value = rows.value.filter((r) => r.key !== key);
}

async function save() {
  if (invalid.value) return;
  saving.value = true;
  saveError.value = null;
  try {
    const body: Expectation[] = rows.value.map(
      ({ kind, scanType, maxAgeDays }) => ({ kind, scanType, maxAgeDays }),
    );
    exp.data.value = await api.put<Expectation[]>("/api/v1/expectations", body);
    resetRows();
    toasts.ok("Expectations saved. Coverage recalculated.");
    cov.run();
  } catch (e) {
    saveError.value = errorMessage(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div>
    <PageHeader title="Coverage" eyebrow="Inventory">
      Which assets are actually being scanned. Each asset kind expects certain
      scan types within a maximum age; anything missing or older is a gap.
    </PageHeader>

    <LoadingBlock
      v-if="cov.loading.value && !cov.data.value"
      :rows="4"
      label="Loading coverage"
    />
    <ErrorBlock
      v-else-if="cov.error.value && !cov.data.value"
      :message="cov.error.value"
      @retry="cov.run()"
    />
    <template v-else-if="cov.data.value">
      <!-- Score -->
      <section
        class="relative isolate grid grid-cols-1 gap-8 overflow-hidden rounded-md border border-ink-700 px-4 py-6 sm:px-6 sm:py-7 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)]"
      >
        <SearchBeam :angle="28" :intensity="0.45" class="-z-10" />
        <div>
          <p class="eyebrow">Overall score</p>
          <p
            class="mt-1 font-display text-[5rem] leading-none font-extrabold text-fog-100 num"
            data-testid="coverage-score"
          >
            {{ pct(cov.data.value.score) }}
          </p>
          <ScoreMeter
            :score="cov.data.value.score"
            size="lg"
            class="mt-4"
            label="Overall coverage"
          />
          <p class="mt-3 text-sm text-fog-400">
            Fresh scans divided by expected scans, across all active assets.
            <span v-if="cov.data.value.gaps.length" class="text-warn"
              >{{ plural(cov.data.value.gaps.length, "gap") }} to close.</span
            >
            <span v-else class="text-ok">No gaps.</span>
          </p>
        </div>
        <div>
          <p class="eyebrow mb-2">By project</p>
          <p
            v-if="!cov.data.value.byProject.length"
            class="text-sm text-fog-500"
          >
            No assets to score yet.
          </p>
          <table v-else class="dtable">
            <thead>
              <tr>
                <th>Project</th>
                <th class="w-[40%]">Score</th>
                <th class="text-right">Assets</th>
                <th class="text-right">Gaps</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="p in cov.data.value.byProject"
                :key="String(p.projectId)"
                data-testid="coverage-project"
              >
                <td
                  :class="
                    p.projectId === null
                      ? 'text-fog-400 italic'
                      : 'text-fog-100'
                  "
                >
                  {{ p.projectId === null ? "Unassigned" : p.name }}
                </td>
                <td>
                  <div class="flex items-center gap-3">
                    <ScoreMeter
                      :score="p.score"
                      :label="`Coverage for ${p.name}`"
                      class="flex-1"
                    />
                    <span class="num w-10 text-right text-xs text-fog-200">{{
                      pct(p.score)
                    }}</span>
                  </div>
                </td>
                <td class="num text-right">
                  <RouterLink
                    v-if="p.projectId !== null"
                    :to="{
                      path: '/assets',
                      query: { project: String(p.projectId) },
                    }"
                    class="hover:text-beam"
                    >{{ p.assets }}</RouterLink
                  >
                  <span v-else>{{ p.assets }}</span>
                </td>
                <td
                  class="num text-right"
                  :class="p.gaps ? 'text-warn' : 'text-fog-500'"
                >
                  {{ p.gaps }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- Gaps -->
      <section class="mt-10">
        <h2 class="mb-3 text-xl text-fog-100">Gaps</h2>
        <EmptyState
          v-if="!sortedGaps.length"
          icon="circle-check"
          title="Fully covered"
          compact
        >
          Every active asset has every expected scan type within its maximum
          age. New assets and stale scans will show up here.
        </EmptyState>
        <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
          <table class="dtable min-w-[40rem]">
            <thead>
              <tr>
                <th>Asset</th>
                <th>Missing scan</th>
                <th>Last seen</th>
                <th class="text-right">Max age</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="g in sortedGaps"
                :key="`${g.assetId}/${g.scanType}`"
                data-testid="coverage-gap"
              >
                <td>
                  <RouterLink
                    :to="`/assets/${g.assetId}`"
                    class="inline-flex items-center gap-2 text-fog-100 hover:text-beam"
                  >
                    <AppIcon
                      :name="assetKindIcon[g.assetKind]"
                      :size="14"
                      class="text-fog-400"
                      :label="assetKindLabel[g.assetKind]"
                    />{{ g.assetName }}
                  </RouterLink>
                </td>
                <td>
                  <span class="tag !border-warn/50 !text-warn">{{
                    scanTypeLabel[g.scanType]
                  }}</span>
                </td>
                <td
                  class="num"
                  :class="g.lastAt ? 'text-fog-200' : 'text-warn'"
                  :title="dateTime(g.lastAt)"
                >
                  {{ g.lastAt ? ago(g.lastAt) : "never" }}
                </td>
                <td class="num text-right text-fog-400">
                  {{ g.maxAgeDays }} {{ g.maxAgeDays === 1 ? "day" : "days" }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </template>

    <!-- Expectations -->
    <section class="mt-10 max-w-3xl">
      <div class="mb-1 flex flex-wrap items-baseline justify-between gap-3">
        <h2 class="text-xl text-fog-100">Expectations</h2>
        <p v-if="dirty" class="text-xs text-beam">Unsaved changes</p>
      </div>
      <p class="mb-4 text-sm text-fog-400">
        What each kind of asset should be scanned for, and how old the latest
        scan may be.
      </p>

      <LoadingBlock
        v-if="exp.loading.value && !exp.data.value"
        :rows="3"
        label="Loading expectations"
      />
      <ErrorBlock
        v-else-if="exp.error.value && !exp.data.value"
        :message="exp.error.value"
        @retry="loadExpectations"
      />
      <form v-else @submit.prevent="save">
        <EmptyState
          v-if="!rows.length"
          icon="radar"
          title="No expectations"
          compact
        >
          Without expectations every asset counts as covered. Add a row, for
          example container images need a vulnerability scan every 7 days.
        </EmptyState>
        <table v-else class="dtable">
          <thead>
            <tr>
              <th>Asset kind</th>
              <th>Scan type</th>
              <th>Max age (days)</th>
              <th class="w-px"><span class="sr-only">Remove</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(r, i) in rows"
              :key="r.key"
              data-testid="expectation-row"
            >
              <td>
                <label :for="`exp-kind-${r.key}`" class="sr-only"
                  >Asset kind, row {{ i + 1 }}</label
                >
                <div class="select-wrap">
                  <select
                    :id="`exp-kind-${r.key}`"
                    v-model="r.kind"
                    class="select !h-7"
                    data-testid="expectation-kind"
                  >
                    <option v-for="k in ASSET_KINDS" :key="k" :value="k">
                      {{ assetKindLabel[k] }}
                    </option>
                  </select>
                </div>
              </td>
              <td>
                <label :for="`exp-scan-${r.key}`" class="sr-only"
                  >Scan type, row {{ i + 1 }}</label
                >
                <div class="select-wrap">
                  <select
                    :id="`exp-scan-${r.key}`"
                    v-model="r.scanType"
                    class="select !h-7"
                    :aria-invalid="dupKeys.has(`${r.kind}/${r.scanType}`)"
                    data-testid="expectation-scan-type"
                  >
                    <option v-for="s in SCAN_TYPES" :key="s" :value="s">
                      {{ scanTypeLabel[s] }}
                    </option>
                  </select>
                </div>
                <p
                  v-if="dupKeys.has(`${r.kind}/${r.scanType}`)"
                  class="mt-1 text-[0.7rem] text-[#ff8a99]"
                >
                  Duplicate
                </p>
              </td>
              <td>
                <label :for="`exp-age-${r.key}`" class="sr-only"
                  >Max age in days, row {{ i + 1 }}</label
                >
                <input
                  :id="`exp-age-${r.key}`"
                  v-model.number="r.maxAgeDays"
                  type="number"
                  min="1"
                  max="3650"
                  step="1"
                  class="input num !h-7 w-24"
                  data-testid="expectation-max-age"
                />
              </td>
              <td>
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  :aria-label="`Remove row ${i + 1}`"
                  @click="removeRow(r.key)"
                >
                  <AppIcon name="x" :size="14" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="saveError" role="alert" class="mt-3 text-sm text-[#ff8a99]">
          {{ saveError }}
        </p>
        <div class="mt-3 flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="btn"
            data-testid="expectation-add"
            @click="addRow"
          >
            <AppIcon name="plus" :size="14" />Add row
          </button>
          <span class="flex-1" />
          <button
            v-if="dirty"
            type="button"
            class="btn btn-ghost"
            @click="resetRows"
          >
            Discard
          </button>
          <button
            type="submit"
            class="btn btn-primary"
            :disabled="!dirty || invalid || saving"
            data-testid="expectation-save"
          >
            {{ saving ? "Saving…" : "Save expectations" }}
          </button>
        </div>
      </form>
    </section>
  </div>
</template>
