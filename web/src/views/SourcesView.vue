<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import AppIcon from "../components/AppIcon.vue";
import PageHeader from "../components/PageHeader.vue";
import EmptyState from "../components/EmptyState.vue";
import LoadingBlock from "../components/LoadingBlock.vue";
import ErrorBlock from "../components/ErrorBlock.vue";
import ConfirmInline from "../components/ConfirmInline.vue";
import ModalDialog from "../components/ModalDialog.vue";
import CopyButton from "../components/CopyButton.vue";
import CommandBlock from "../components/CommandBlock.vue";
import { api, errorMessage } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ago, dateTime, plural } from "../lib/format";
import type { Source } from "../lib/types";
import { useToasts } from "../stores/toasts";

const toasts = useToasts();
const sources = useAsync(() => api.get<Source[]>("/api/v1/sources"));
onMounted(() => sources.run());

const staleCount = computed(
  () => (sources.data.value ?? []).filter((s) => s.stale).length,
);

// --- create / edit ------------------------------------------------------------
const formOpen = ref(false);
const editing = ref<Source | null>(null);
const form = reactive({ name: "", staleAfterHours: 48 });
const formBusy = ref(false);
const formError = ref<string | null>(null);

function openNew() {
  editing.value = null;
  form.name = "";
  form.staleAfterHours = 48;
  formError.value = null;
  formOpen.value = true;
}
function openEdit(s: Source) {
  editing.value = s;
  form.name = s.name;
  form.staleAfterHours = s.staleAfterHours;
  formError.value = null;
  formOpen.value = true;
}

async function saveForm() {
  formError.value = null;
  if (!form.name.trim()) {
    formError.value = "A name is required.";
    return;
  }
  if (!Number.isInteger(form.staleAfterHours) || form.staleAfterHours < 1) {
    formError.value =
      "The stale threshold must be a whole number of hours, at least 1.";
    return;
  }
  formBusy.value = true;
  try {
    if (editing.value) {
      await api.patch(`/api/v1/sources/${editing.value.id}`, {
        name: form.name.trim(),
        staleAfterHours: form.staleAfterHours,
      });
      toasts.ok("Source saved.");
      formOpen.value = false;
    } else {
      const created = await api.post<Source & { token: string }>(
        "/api/v1/sources",
        {
          name: form.name.trim(),
          staleAfterHours: form.staleAfterHours,
        },
      );
      formOpen.value = false;
      showToken(created.name, created.token, false);
    }
    sources.run();
  } catch (e) {
    formError.value = errorMessage(e);
  } finally {
    formBusy.value = false;
  }
}

// --- token reveal ---------------------------------------------------------------
const token = ref<{ name: string; value: string; rotated: boolean } | null>(
  null,
);
function showToken(name: string, value: string, rotated: boolean) {
  token.value = { name, value, rotated };
}
const origin = location.origin;
const exampleImage = "ghcr.io/acme/api:latest";
const exampleScan = computed(
  () => `trivy image --format json --output report.json ${exampleImage}`,
);
const exampleCurl = computed(
  () =>
    `curl -X POST "${origin}/api/v1/ingest?format=trivy&asset=image:${exampleImage.replace(/:[^:/]+$/, "")}" \\\n` +
    `  -H "Authorization: Bearer ${token.value?.value ?? "<token>"}" \\\n` +
    `  --data-binary @report.json`,
);

// --- rotate / delete ------------------------------------------------------------
const busyId = ref<number | null>(null);
async function rotate(s: Source) {
  busyId.value = s.id;
  try {
    const r = await api.post<{ token: string }>(
      `/api/v1/sources/${s.id}/rotate`,
    );
    showToken(s.name, r.token, true);
    sources.run();
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    busyId.value = null;
  }
}
async function remove(s: Source) {
  busyId.value = s.id;
  try {
    await api.del(`/api/v1/sources/${s.id}`);
    toasts.ok(`Deleted source ${s.name}. Its token no longer works.`);
    sources.run();
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    busyId.value = null;
  }
}

function hours(h: number) {
  if (h % 24 === 0) return plural(h / 24, "day");
  return plural(h, "hour");
}
</script>

<template>
  <div>
    <PageHeader title="Sources" eyebrow="Setup">
      A source is a CI job or cron that pushes scanner reports with its own
      token. If a source stops reporting for longer than its threshold, it is
      flagged as quiet.
      <template #actions>
        <button
          type="button"
          class="btn btn-primary"
          data-testid="source-new"
          @click="openNew"
        >
          <AppIcon name="plus" :size="14" />New source
        </button>
      </template>
    </PageHeader>

    <LoadingBlock
      v-if="sources.loading.value && !sources.data.value"
      :rows="4"
      label="Loading sources"
    />
    <ErrorBlock
      v-else-if="sources.error.value"
      :message="sources.error.value"
      @retry="sources.run()"
    />
    <template v-else-if="sources.data.value">
      <EmptyState
        v-if="!sources.data.value.length"
        icon="radio-tower"
        title="No sources yet"
      >
        Create a source to get an ingest token, then push Trivy or SARIF reports
        from CI. Findings, assets and coverage all start here.
        <template #actions>
          <button type="button" class="btn btn-primary" @click="openNew">
            <AppIcon name="plus" :size="14" />Create your first source
          </button>
        </template>
      </EmptyState>
      <template v-else>
        <p
          v-if="staleCount"
          class="mb-3 flex items-center gap-2 text-sm text-warn"
          role="status"
        >
          <AppIcon name="triangle-alert" :size="15" />{{
            plural(staleCount, "source")
          }}
          went quiet.
        </p>
        <div class="-mx-4 overflow-x-auto sm:mx-0">
          <table class="dtable min-w-[52rem]">
            <thead>
              <tr>
                <th>Name</th>
                <th>Token</th>
                <th>Last seen</th>
                <th>Quiet after</th>
                <th class="text-right">Scans</th>
                <th class="w-px"><span class="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="s in sources.data.value"
                :key="s.id"
                data-testid="source-row"
                :data-name="s.name"
              >
                <td>
                  <span class="font-medium text-fog-100">{{ s.name }}</span>
                  <p class="text-[0.7rem] text-fog-500">
                    created {{ ago(s.createdAt) }}
                  </p>
                </td>
                <td>
                  <code class="font-mono text-xs text-fog-300"
                    >{{ s.tokenPrefix }}…</code
                  >
                </td>
                <td class="whitespace-nowrap">
                  <span
                    class="num"
                    :class="s.stale ? 'text-warn' : 'text-fog-200'"
                    :title="dateTime(s.lastSeenAt)"
                    >{{ ago(s.lastSeenAt) }}</span
                  >
                  <span
                    v-if="s.stale"
                    class="ml-2 inline-flex h-5 items-center gap-1 rounded-[3px] border border-warn/50 bg-warn/10 px-1.5 text-[0.68rem] font-medium text-warn"
                    data-testid="source-stale"
                  >
                    <AppIcon name="triangle-alert" :size="11" />quiet
                  </span>
                </td>
                <td class="num text-fog-300">{{ hours(s.staleAfterHours) }}</td>
                <td class="num text-right">{{ s.scans }}</td>
                <td>
                  <div class="flex justify-end">
                    <ConfirmInline
                      :question="`Delete ${s.name}? CI using its token will be rejected.`"
                      :busy="busyId === s.id"
                      testid="source-delete-confirm"
                      @confirm="remove(s)"
                    >
                      <template #default="{ ask }">
                        <button
                          type="button"
                          class="btn btn-ghost btn-sm"
                          :aria-label="`Edit ${s.name}`"
                          data-testid="source-edit"
                          @click="openEdit(s)"
                        >
                          <AppIcon name="pencil" :size="13" />
                        </button>
                        <ConfirmInline
                          :question="`Rotate the token? The old one stops working now.`"
                          confirm-label="Rotate"
                          :busy="busyId === s.id"
                          testid="source-rotate-confirm"
                          @confirm="rotate(s)"
                        >
                          <template #default="{ ask: askRotate }">
                            <button
                              type="button"
                              class="btn btn-ghost btn-sm"
                              :aria-label="`Rotate token for ${s.name}`"
                              title="Rotate token"
                              data-testid="source-rotate"
                              @click="askRotate"
                            >
                              <AppIcon name="key-round" :size="13" />
                            </button>
                          </template>
                        </ConfirmInline>
                        <button
                          type="button"
                          class="btn btn-ghost btn-sm"
                          :aria-label="`Delete ${s.name}`"
                          data-testid="source-delete"
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
    </template>

    <ModalDialog
      :open="formOpen"
      :title="editing ? 'Edit source' : 'New source'"
      testid="source-dialog"
      @close="formOpen = false"
    >
      <form id="source-form" class="space-y-4" @submit.prevent="saveForm">
        <div>
          <label for="source-name" class="field-label">Name</label>
          <input
            id="source-name"
            v-model="form.name"
            class="input"
            placeholder="e.g. github-actions-api"
            required
            data-testid="source-name"
            data-autofocus
          />
          <p class="field-hint">
            Shown on findings as the reporter. One source per pipeline keeps
            "went quiet" alerts precise.
          </p>
        </div>
        <div>
          <label for="source-stale" class="field-label"
            >Quiet after (hours)</label
          >
          <input
            id="source-stale"
            v-model.number="form.staleAfterHours"
            type="number"
            min="1"
            step="1"
            class="input num w-32"
            data-testid="source-stale-hours"
          />
          <p class="field-hint">
            Flag the source if no report arrives for this long. Default 48.
          </p>
        </div>
        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">
          {{ formError }}
        </p>
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost" @click="formOpen = false">
          Cancel
        </button>
        <button
          type="submit"
          form="source-form"
          class="btn btn-primary"
          :disabled="formBusy"
          data-testid="source-save"
        >
          {{ formBusy ? "Saving…" : editing ? "Save" : "Create source" }}
        </button>
      </template>
    </ModalDialog>

    <ModalDialog
      :open="!!token"
      :title="token?.rotated ? 'New token' : 'Source created'"
      width="40rem"
      testid="token-dialog"
      @close="token = null"
    >
      <template v-if="token">
        <p
          class="flex items-start gap-2 rounded-[4px] border border-beam/40 bg-beam/[0.07] px-3 py-2 text-sm text-fog-100"
        >
          <AppIcon name="key-round" :size="15" class="mt-0.5 text-beam" />
          <span
            >This is the only time the token for
            <strong>{{ token.name }}</strong> is shown. Store it as a CI secret
            now.</span
          >
        </p>
        <label class="field-label mt-4" for="source-token-value">Token</label>
        <div class="flex items-stretch gap-2">
          <code
            id="source-token-value"
            class="codeblock min-w-0 flex-1 !border-l-beam select-all"
            data-testid="source-token"
            >{{ token.value }}</code
          >
          <CopyButton :text="token.value" class="self-center" data-autofocus />
        </div>

        <p class="eyebrow mt-6 mb-2">Push your first report</p>
        <ol class="space-y-3 text-sm text-fog-300">
          <li>
            <p class="mb-1">1. Scan, for example a container image:</p>
            <CommandBlock :command="exampleScan" />
          </li>
          <li>
            <p class="mb-1">
              2. Send the report. Change
              <code class="text-fog-100">asset=</code> to match what you
              scanned:
            </p>
            <CommandBlock :command="exampleCurl" testid="source-example" />
          </li>
        </ol>
        <p class="field-hint mt-3">
          Use <code>format=sarif</code> for SARIF. Assets that do not exist yet
          are created on first ingest.
        </p>
      </template>
      <template #footer>
        <button type="button" class="btn btn-primary" @click="token = null">
          I have stored the token
        </button>
      </template>
    </ModalDialog>
  </div>
</template>
