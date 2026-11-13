<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import PageHeader from "../components/PageHeader.vue";
import ChannelsPanel from "../components/notifications/ChannelsPanel.vue";
import RulesPanel from "../components/notifications/RulesPanel.vue";
import LogPanel from "../components/notifications/LogPanel.vue";

type Tab = "channels" | "rules" | "log";
const TABS: { id: Tab; label: string }[] = [
  { id: "channels", label: "Channels" },
  { id: "rules", label: "Rules" },
  { id: "log", label: "Log" },
];

const route = useRoute();
const router = useRouter();
const parse = (v: unknown): Tab =>
  v === "rules" || v === "log" ? v : "channels";
const tab = ref<Tab>(parse(route.query.tab));

watch(tab, (t) =>
  router.replace({ query: t === "channels" ? {} : { tab: t } }),
);
watch(
  () => route.query.tab,
  (v) => (tab.value = parse(v)),
);

const idx = computed(() => TABS.findIndex((t) => t.id === tab.value));
function onTabKey(e: KeyboardEvent) {
  let i = idx.value;
  if (e.key === "ArrowRight") i = (i + 1) % TABS.length;
  else if (e.key === "ArrowLeft") i = (i - 1 + TABS.length) % TABS.length;
  else if (e.key === "Home") i = 0;
  else if (e.key === "End") i = TABS.length - 1;
  else return;
  e.preventDefault();
  tab.value = TABS[i].id;
  document.getElementById(`tab-${TABS[i].id}`)?.focus();
}
</script>

<template>
  <div>
    <PageHeader title="Notifications" eyebrow="Setup">
      Channels say where alerts go; rules say what is worth an alert. Digests go
      out at 08:00 UTC, weekly ones on Mondays.
    </PageHeader>

    <div
      role="tablist"
      aria-label="Notification settings"
      class="mb-6 flex gap-1 border-b border-ink-700"
      @keydown="onTabKey"
    >
      <button
        v-for="t in TABS"
        :id="`tab-${t.id}`"
        :key="t.id"
        type="button"
        role="tab"
        :aria-selected="tab === t.id"
        :aria-controls="`panel-${t.id}`"
        :tabindex="tab === t.id ? 0 : -1"
        class="tabbtn"
        :class="tab === t.id ? 'is-active' : ''"
        :data-testid="`tab-${t.id}`"
        @click="tab = t.id"
      >
        {{ t.label }}
      </button>
    </div>

    <div :id="`panel-${tab}`" role="tabpanel" :aria-labelledby="`tab-${tab}`">
      <ChannelsPanel v-if="tab === 'channels'" />
      <RulesPanel
        v-else-if="tab === 'rules'"
        @need-channel="tab = 'channels'"
      />
      <LogPanel v-else />
    </div>
  </div>
</template>

<style scoped>
.tabbtn {
  position: relative;
  height: 2.4rem;
  padding: 0 0.9rem;
  font-family: var(--font-display);
  font-size: 0.95rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-fog-400);
  cursor: pointer;
}
.tabbtn:hover {
  color: var(--color-fog-100);
}
.tabbtn.is-active {
  color: var(--color-fog-100);
}
.tabbtn.is-active::after {
  content: "";
  position: absolute;
  left: 0.5rem;
  right: 0.5rem;
  bottom: -1px;
  height: 2px;
  background: var(--color-beam);
  box-shadow: 0 0 10px var(--color-beam);
}
</style>
