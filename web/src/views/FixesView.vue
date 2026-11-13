<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import AppIcon from "../components/AppIcon.vue";
import PageHeader from "../components/PageHeader.vue";
import FixGroupItem from "../components/FixGroupItem.vue";
import CopyButton from "../components/CopyButton.vue";
import EmptyState from "../components/EmptyState.vue";
import LoadingBlock from "../components/LoadingBlock.vue";
import ErrorBlock from "../components/ErrorBlock.vue";
import PriorityChip from "../components/PriorityChip.vue";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { plural } from "../lib/format";
import type { FixGroup, Priority } from "../lib/types";
import { useProjects } from "../stores/projects";

const route = useRoute();
const router = useRouter();
const projects = useProjects();

const project = ref<number | null>(Number(route.query.project) || null);
const fixes = useAsync(() =>
  api.get<FixGroup[]>("/api/v1/fixes", { project: project.value ?? undefined }),
);

onMounted(() => {
  projects.ensure();
  fixes.run();
});
watch(project, (p) => {
  router.replace({ query: p ? { project: String(p) } : {} });
  fixes.run();
});

const byAsset = computed(() => {
  const groups = new Map<
    number,
    {
      assetId: number;
      assetName: string;
      items: FixGroup[];
      findings: number;
      maxPriority: Priority;
      kev: boolean;
    }
  >();
  for (const g of fixes.data.value ?? []) {
    let a = groups.get(g.assetId);
    if (!a) {
      a = {
        assetId: g.assetId,
        assetName: g.assetName,
        items: [],
        findings: 0,
        maxPriority: g.maxPriority,
        kev: false,
      };
      groups.set(g.assetId, a);
    }
    a.items.push(g);
    a.findings += g.findings;
    a.kev ||= g.kev;
    if (g.maxPriority < a.maxPriority) a.maxPriority = g.maxPriority;
  }
  return [...groups.values()];
});
const totalFindings = computed(() =>
  (fixes.data.value ?? []).reduce((s, g) => s + g.findings, 0),
);
</script>

<template>
  <div>
    <PageHeader title="Fixes" eyebrow="Upgrades">
      Each line is one upgrade that closes one or more open findings. Copy the
      command, ship it, and the next scan closes the findings.
      <template #actions>
        <label for="fix-project" class="sr-only">Project</label>
        <div class="select-wrap">
          <select id="fix-project" v-model="project" class="select w-auto">
            <option :value="null">All projects</option>
            <option v-for="p in projects.list" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select>
        </div>
      </template>
    </PageHeader>

    <LoadingBlock
      v-if="fixes.loading.value && !fixes.data.value"
      :rows="6"
      label="Loading fixes"
    />
    <ErrorBlock
      v-else-if="fixes.error.value"
      :message="fixes.error.value"
      @retry="fixes.run()"
    />
    <template v-else-if="fixes.data.value">
      <EmptyState
        v-if="!byAsset.length"
        icon="wrench"
        :title="project ? 'No fixes in this project' : 'No fixes available'"
      >
        When an open finding has a fixed version upstream, the upgrade that
        closes it appears here, grouped by asset, with a copy-ready command.
        <template #actions
          ><RouterLink to="/findings" class="btn"
            >Open the inbox</RouterLink
          ></template
        >
      </EmptyState>
      <template v-else>
        <p class="mb-4 text-sm text-fog-400">
          <span class="num text-fog-100">{{
            plural(fixes.data.value.length, "upgrade")
          }}</span>
          across
          <span class="num text-fog-100">{{
            plural(byAsset.length, "asset")
          }}</span>
          would close
          <span class="num text-fog-100">{{
            plural(totalFindings, "finding")
          }}</span
          >.
        </p>
        <section
          v-for="a in byAsset"
          :key="a.assetId"
          class="mb-8 border-t border-ink-600"
          :aria-label="a.assetName"
        >
          <header class="flex flex-wrap items-center gap-3 pt-3">
            <PriorityChip :p="a.maxPriority" />
            <h2
              class="min-w-0 truncate text-xl text-fog-100 normal-case tracking-wide"
            >
              <RouterLink
                :to="`/assets/${a.assetId}`"
                class="hover:text-beam"
                >{{ a.assetName }}</RouterLink
              >
            </h2>
            <span
              v-if="a.kev"
              class="inline-flex items-center gap-0.5 font-mono text-[0.68rem] font-bold text-[#ff8a99]"
              ><AppIcon name="flame" :size="11" />KEV</span
            >
            <span class="text-xs text-fog-400"
              >{{ plural(a.items.length, "upgrade") }} ·
              {{ plural(a.findings, "finding") }}</span
            >
            <CopyButton
              v-if="a.items.length > 1"
              class="ml-auto"
              :text="a.items.map((i) => i.command).join('\n')"
              label="Copy all"
            />
          </header>
          <ul class="divide-y divide-ink-700">
            <FixGroupItem
              v-for="g in a.items"
              :key="g.key"
              :group="g"
              :max-advisories="6"
            />
          </ul>
        </section>
      </template>
    </template>
  </div>
</template>
