<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import AppIcon from "../AppIcon.vue";
import EmptyState from "../EmptyState.vue";
import LoadingBlock from "../LoadingBlock.vue";
import ErrorBlock from "../ErrorBlock.vue";
import ConfirmInline from "../ConfirmInline.vue";
import ModalDialog from "../ModalDialog.vue";
import { api, errorMessage } from "../../lib/api";
import { useAsync } from "../../lib/useAsync";
import { parseTags } from "../../lib/format";
import {
  PRIORITIES,
  TRIGGERS,
  maxPriorityLabel,
  modeLabel,
  triggerLabel,
} from "../../lib/labels";
import type {
  Channel,
  Priority,
  Rule,
  RuleInput,
  RuleMode,
  Trigger,
} from "../../lib/types";
import { useProjects } from "../../stores/projects";
import { useToasts } from "../../stores/toasts";

const emit = defineEmits<{ "need-channel": [] }>();
const toasts = useToasts();
const projects = useProjects();

const rules = useAsync(() => api.get<Rule[]>("/api/v1/rules"));
const channels = useAsync(() => api.get<Channel[]>("/api/v1/channels"));
onMounted(() => {
  rules.run();
  channels.run();
  projects.ensure();
});

const triggerHint: Record<Trigger, string> = {
  new_finding: "A scan reports a finding that was not open before.",
  risk_increase:
    "An open finding moves to a more urgent priority, e.g. it lands in CISA KEV.",
  stale_source: "A source has not reported for longer than its threshold.",
  accepted_expired:
    "An accepted risk passes its end date and returns to the inbox.",
  coverage_gap:
    "An asset misses an expected scan type or its last scan got too old.",
};
const MODES: RuleMode[] = ["instant", "daily", "weekly"];

function summary(r: Rule): string {
  const parts: string[] = [];
  if (r.trigger !== "stale_source")
    parts.push(
      maxPriorityLabel[r.filter.maxPriority] ?? `P1–P${r.filter.maxPriority}`,
    );
  if (r.filter.projectId !== null)
    parts.push(
      `project ${projects.nameOf(r.filter.projectId) ?? "#" + r.filter.projectId}`,
    );
  if (r.filter.tags?.length) parts.push(`tags ${r.filter.tags.join(", ")}`);
  if (r.trigger !== "stale_source" && r.filter.fixAvailable !== null)
    parts.push(r.filter.fixAvailable ? "fix available" : "no fix");
  return parts.join(" · ") || "everything";
}

// --- form -------------------------------------------------------------------------
const formOpen = ref(false);
const editing = ref<Rule | null>(null);
const form = reactive({
  name: "",
  trigger: "new_finding" as Trigger,
  maxPriority: 2 as Priority,
  projectId: null as number | null,
  tags: "",
  fixAvailable: "any" as "any" | "yes" | "no",
  mode: "instant" as RuleMode,
  channelId: null as number | null,
  enabled: true,
});
const busy = ref(false);
const formError = ref<string | null>(null);
const priorityApplies = computed(() => form.trigger !== "stale_source");

function openNew() {
  editing.value = null;
  Object.assign(form, {
    name: "",
    trigger: "new_finding",
    maxPriority: 2,
    projectId: null,
    tags: "",
    fixAvailable: "any",
    mode: "instant",
    channelId: channels.data.value?.[0]?.id ?? null,
    enabled: true,
  });
  formError.value = null;
  formOpen.value = true;
}
function openEdit(r: Rule) {
  editing.value = r;
  Object.assign(form, {
    name: r.name,
    trigger: r.trigger,
    maxPriority: r.filter.maxPriority,
    projectId: r.filter.projectId,
    tags: (r.filter.tags ?? []).join(", "),
    fixAvailable:
      r.filter.fixAvailable === null
        ? "any"
        : r.filter.fixAvailable
          ? "yes"
          : "no",
    mode: r.mode,
    channelId: r.channelId,
    enabled: r.enabled,
  });
  formError.value = null;
  formOpen.value = true;
}

async function save() {
  formError.value = null;
  if (!form.name.trim()) return void (formError.value = "A name is required.");
  if (form.channelId === null)
    return void (formError.value = "Pick a channel.");
  const body: RuleInput = {
    name: form.name.trim(),
    trigger: form.trigger,
    filter: {
      maxPriority: form.maxPriority,
      projectId: form.projectId,
      tags: parseTags(form.tags),
      fixAvailable:
        form.fixAvailable === "any" ? null : form.fixAvailable === "yes",
    },
    mode: form.mode,
    channelId: form.channelId,
    enabled: form.enabled,
  };
  busy.value = true;
  try {
    if (editing.value)
      await api.patch(`/api/v1/rules/${editing.value.id}`, body);
    else await api.post("/api/v1/rules", body);
    toasts.ok(editing.value ? "Rule saved." : "Rule created.");
    formOpen.value = false;
    rules.run();
  } catch (e) {
    formError.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

// --- toggle / delete ----------------------------------------------------------------
const busyId = ref<number | null>(null);
async function toggle(r: Rule) {
  busyId.value = r.id;
  try {
    await api.patch(`/api/v1/rules/${r.id}`, { enabled: !r.enabled });
    await rules.run();
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    busyId.value = null;
  }
}
async function remove(r: Rule) {
  busyId.value = r.id;
  try {
    await api.del(`/api/v1/rules/${r.id}`);
    toasts.ok(`Deleted rule ${r.name}.`);
    rules.run();
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    busyId.value = null;
  }
}

const noChannels = computed(
  () => channels.data.value !== null && channels.data.value.length === 0,
);
</script>

<template>
  <div>
    <div class="mb-4 flex items-center justify-between gap-3">
      <p class="text-sm text-fog-400">What is worth an alert, and how often.</p>
      <button
        type="button"
        class="btn btn-primary"
        data-testid="rule-new"
        @click="openNew"
      >
        <AppIcon name="plus" :size="14" />New rule
      </button>
    </div>

    <LoadingBlock
      v-if="rules.loading.value && !rules.data.value"
      :rows="3"
      label="Loading rules"
    />
    <ErrorBlock
      v-else-if="rules.error.value"
      :message="rules.error.value"
      @retry="rules.run()"
    />
    <template v-else-if="rules.data.value">
      <EmptyState
        v-if="!rules.data.value.length"
        icon="list-checks"
        title="No rules yet"
      >
        Rules turn events into alerts, for example "every new P1 finding,
        instantly, to the security channel" or "a weekly digest of everything".
        <template v-if="noChannels"> Start by adding a channel.</template>
        <template #actions>
          <button
            v-if="noChannels"
            type="button"
            class="btn btn-primary"
            @click="emit('need-channel')"
          >
            Add a channel first
          </button>
          <button v-else type="button" class="btn btn-primary" @click="openNew">
            <AppIcon name="plus" :size="14" />Create a rule
          </button>
        </template>
      </EmptyState>
      <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
        <table class="dtable min-w-[48rem]">
          <thead>
            <tr>
              <th class="w-16">On</th>
              <th>Rule</th>
              <th>Filter</th>
              <th>Delivery</th>
              <th class="w-px"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="r in rules.data.value"
              :key="r.id"
              data-testid="rule-row"
              :data-name="r.name"
              :class="r.enabled ? '' : 'opacity-60'"
            >
              <td>
                <button
                  type="button"
                  role="switch"
                  class="switch"
                  :class="r.enabled ? 'is-on' : ''"
                  :aria-checked="r.enabled"
                  :aria-label="`${r.enabled ? 'Disable' : 'Enable'} ${r.name}`"
                  :disabled="busyId === r.id"
                  data-testid="rule-toggle"
                  @click="toggle(r)"
                />
              </td>
              <td>
                <p class="font-medium text-fog-100">{{ r.name }}</p>
                <p class="text-xs text-fog-400">
                  {{ triggerLabel[r.trigger] ?? r.trigger }}
                </p>
              </td>
              <td class="text-xs text-fog-300">{{ summary(r) }}</td>
              <td class="text-xs whitespace-nowrap">
                <p class="text-fog-200">{{ modeLabel[r.mode] }}</p>
                <p class="text-fog-400">to {{ r.channelName }}</p>
              </td>
              <td>
                <div class="flex justify-end">
                  <ConfirmInline
                    :question="`Delete ${r.name}?`"
                    :busy="busyId === r.id"
                    @confirm="remove(r)"
                  >
                    <template #default="{ ask }">
                      <button
                        type="button"
                        class="btn btn-ghost btn-sm"
                        :aria-label="`Edit ${r.name}`"
                        data-testid="rule-edit"
                        @click="openEdit(r)"
                      >
                        <AppIcon name="pencil" :size="13" />
                      </button>
                      <button
                        type="button"
                        class="btn btn-ghost btn-sm"
                        :aria-label="`Delete ${r.name}`"
                        data-testid="rule-delete"
                        @click="ask"
                      >
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

    <ModalDialog
      :open="formOpen"
      :title="editing ? 'Edit rule' : 'New rule'"
      width="34rem"
      testid="rule-dialog"
      @close="formOpen = false"
    >
      <div v-if="noChannels" class="space-y-3 text-sm text-fog-300">
        <p>A rule needs a channel to deliver to, and there are none yet.</p>
        <button
          type="button"
          class="btn btn-primary"
          @click="((formOpen = false), emit('need-channel'))"
        >
          Add a channel
        </button>
      </div>
      <form v-else id="rule-form" class="space-y-4" @submit.prevent="save">
        <div>
          <label for="rule-name" class="field-label">Name</label>
          <input
            id="rule-name"
            v-model="form.name"
            class="input"
            placeholder="e.g. Urgent to on-call"
            required
            data-testid="rule-name"
            data-autofocus
          />
        </div>
        <div>
          <label for="rule-trigger" class="field-label">When</label>
          <div class="select-wrap">
            <select
              id="rule-trigger"
              v-model="form.trigger"
              class="select"
              data-testid="rule-trigger"
            >
              <option v-for="t in TRIGGERS" :key="t" :value="t">
                {{ triggerLabel[t] }}
              </option>
            </select>
          </div>
          <p class="field-hint">{{ triggerHint[form.trigger] }}</p>
        </div>

        <fieldset
          class="space-y-3 rounded-[4px] border border-ink-700 px-3 pt-2 pb-3"
        >
          <legend class="eyebrow px-1 !text-[0.7rem]">Only if</legend>
          <div class="grid gap-3 sm:grid-cols-2">
            <div>
              <label for="rule-max-priority" class="field-label"
                >Priority</label
              >
              <div class="select-wrap">
                <select
                  id="rule-max-priority"
                  v-model.number="form.maxPriority"
                  class="select"
                  :disabled="!priorityApplies"
                  data-testid="rule-max-priority"
                >
                  <option v-for="p in PRIORITIES" :key="p" :value="p">
                    {{ maxPriorityLabel[p] }}
                  </option>
                </select>
              </div>
            </div>
            <div>
              <label for="rule-fix" class="field-label">Fix available</label>
              <div class="select-wrap">
                <select
                  id="rule-fix"
                  v-model="form.fixAvailable"
                  class="select"
                  :disabled="!priorityApplies"
                  data-testid="rule-fix-available"
                >
                  <option value="any">Either</option>
                  <option value="yes">Only with a fix</option>
                  <option value="no">Only without a fix</option>
                </select>
              </div>
            </div>
            <div>
              <label for="rule-project" class="field-label">Project</label>
              <div class="select-wrap">
                <select
                  id="rule-project"
                  v-model="form.projectId"
                  class="select"
                  data-testid="rule-project"
                >
                  <option :value="null">Any project</option>
                  <option v-for="p in projects.list" :key="p.id" :value="p.id">
                    {{ p.name }}
                  </option>
                </select>
              </div>
            </div>
            <div>
              <label for="rule-tags" class="field-label">Asset tags</label>
              <input
                id="rule-tags"
                v-model="form.tags"
                class="input font-mono"
                placeholder="internet-facing"
                data-testid="rule-tags"
              />
            </div>
          </div>
          <p v-if="!priorityApplies" class="field-hint !mt-1">
            Priority and fix filters do not apply to quiet sources.
          </p>
          <p v-else class="field-hint !mt-1">
            Comma separated asset tags. Leave empty to match every asset.
          </p>
        </fieldset>

        <div class="grid gap-3 sm:grid-cols-2">
          <div>
            <label for="rule-mode" class="field-label">How often</label>
            <div class="select-wrap">
              <select
                id="rule-mode"
                v-model="form.mode"
                class="select"
                data-testid="rule-mode"
              >
                <option v-for="m in MODES" :key="m" :value="m">
                  {{ modeLabel[m] }}
                </option>
              </select>
            </div>
          </div>
          <div>
            <label for="rule-channel" class="field-label">Send to</label>
            <div class="select-wrap">
              <select
                id="rule-channel"
                v-model="form.channelId"
                class="select"
                required
                data-testid="rule-channel"
              >
                <option
                  v-for="c in channels.data.value ?? []"
                  :key="c.id"
                  :value="c.id"
                >
                  {{ c.name }}
                </option>
              </select>
            </div>
          </div>
        </div>
        <label
          class="inline-flex cursor-pointer items-center gap-2 text-sm text-fog-200"
        >
          <input
            v-model="form.enabled"
            type="checkbox"
            class="check"
            data-testid="rule-enabled"
          />Enabled
        </label>
        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">
          {{ formError }}
        </p>
      </form>
      <template v-if="!noChannels" #footer>
        <button type="button" class="btn btn-ghost" @click="formOpen = false">
          Cancel
        </button>
        <button
          type="submit"
          form="rule-form"
          class="btn btn-primary"
          :disabled="busy"
          data-testid="rule-save"
        >
          {{ busy ? "Saving…" : editing ? "Save" : "Create rule" }}
        </button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.switch {
  position: relative;
  width: 2.1rem;
  height: 1.2rem;
  border-radius: 999px;
  background: var(--color-ink-600);
  cursor: pointer;
  transition: background-color 120ms;
}
.switch::after {
  content: "";
  position: absolute;
  top: 0.15rem;
  left: 0.15rem;
  width: 0.9rem;
  height: 0.9rem;
  border-radius: 50%;
  background: var(--color-fog-300);
  transition: transform 140ms;
}
.switch.is-on {
  background: var(--color-beam);
}
.switch.is-on::after {
  transform: translateX(0.9rem);
  background: var(--color-beam-ink);
}
</style>
