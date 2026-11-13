<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import ModalDialog from "./ModalDialog.vue";
import { api, errorMessage } from "../lib/api";
import { parseTags } from "../lib/format";
import {
  ASSET_KINDS,
  assetIdentifierHint,
  assetKindLabel,
} from "../lib/labels";
import type { Asset, AssetCreate, AssetKind, AssetPatch } from "../lib/types";
import { useProjects } from "../stores/projects";
import { useToasts } from "../stores/toasts";

const props = defineProps<{ open: boolean; asset?: Asset | null }>();
const emit = defineEmits<{ close: []; saved: [asset: Asset] }>();
const projects = useProjects();
const toasts = useToasts();

const form = reactive({
  kind: "image" as AssetKind,
  identifier: "",
  name: "",
  projectId: null as number | null,
  tags: "",
});
const busy = ref(false);
const error = ref<string | null>(null);
const editing = computed(() => !!props.asset);

watch(
  () => props.open,
  (o) => {
    if (!o) return;
    projects.ensure();
    error.value = null;
    const a = props.asset;
    form.kind = a?.kind ?? "image";
    form.identifier = a?.identifier ?? "";
    form.name = a?.name ?? "";
    form.projectId = a?.projectId ?? null;
    form.tags = a?.tags.join(", ") ?? "";
  },
  { immediate: true },
);

async function save() {
  error.value = null;
  if (!editing.value && !form.identifier.trim()) {
    error.value = "An identifier is required.";
    return;
  }
  busy.value = true;
  try {
    let saved: Asset;
    if (props.asset) {
      const body: AssetPatch = {
        name: form.name.trim(),
        projectId: form.projectId,
        tags: parseTags(form.tags),
      };
      saved = await api.patch<Asset>(`/api/v1/assets/${props.asset.id}`, body);
      toasts.ok("Asset saved.");
    } else {
      const body: AssetCreate = {
        kind: form.kind,
        identifier: form.identifier.trim(),
        tags: parseTags(form.tags),
      };
      if (form.name.trim()) body.name = form.name.trim();
      if (form.projectId) body.projectId = form.projectId;
      saved = await api.post<Asset>("/api/v1/assets", body);
      toasts.ok("Asset added to the inventory.");
    }
    emit("saved", saved);
    emit("close");
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ModalDialog
    :open="open"
    :title="editing ? 'Edit asset' : 'New asset'"
    testid="asset-dialog"
    @close="emit('close')"
  >
    <form id="asset-form" class="space-y-4" @submit.prevent="save">
      <div class="grid gap-4 sm:grid-cols-[11rem_1fr]">
        <div>
          <label for="asset-kind" class="field-label">Kind</label>
          <div class="select-wrap">
            <select
              id="asset-kind"
              v-model="form.kind"
              class="select"
              :disabled="editing"
              data-testid="asset-kind"
              :data-autofocus="editing ? undefined : ''"
            >
              <option v-for="k in ASSET_KINDS" :key="k" :value="k">
                {{ assetKindLabel[k] }}
              </option>
            </select>
          </div>
        </div>
        <div>
          <label for="asset-identifier" class="field-label"
            >Identifier <span v-if="!editing" class="text-p1">*</span></label
          >
          <input
            id="asset-identifier"
            v-model="form.identifier"
            class="input font-mono"
            :disabled="editing"
            :placeholder="assetIdentifierHint[form.kind]"
            autocomplete="off"
            spellcheck="false"
            required
            data-testid="asset-identifier"
          />
        </div>
      </div>
      <p v-if="!editing" class="field-hint -mt-2">
        Ingest matches on kind and identifier, e.g.
        <code class="text-fog-300"
          >asset={{ form.kind }}:{{
            form.identifier || assetIdentifierHint[form.kind]
          }}</code
        >. Kind and identifier cannot be changed later.
      </p>
      <div>
        <label for="asset-name" class="field-label"
          >Display name <span class="text-fog-500">(optional)</span></label
        >
        <input
          id="asset-name"
          v-model="form.name"
          class="input"
          :placeholder="form.identifier || 'Defaults to the identifier'"
          data-testid="asset-name"
          :data-autofocus="editing ? '' : undefined"
        />
      </div>
      <div>
        <label for="asset-project" class="field-label">Project</label>
        <div class="select-wrap">
          <select
            id="asset-project"
            v-model="form.projectId"
            class="select"
            data-testid="asset-project"
          >
            <option :value="null">No project</option>
            <option v-for="p in projects.list" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select>
        </div>
        <p class="field-hint">
          Manage projects on the
          <RouterLink to="/projects" class="link" @click="emit('close')"
            >Projects</RouterLink
          >
          page.
        </p>
      </div>
      <div>
        <label for="asset-tags" class="field-label">Tags</label>
        <input
          id="asset-tags"
          v-model="form.tags"
          class="input font-mono"
          placeholder="internet-facing, team-payments"
          data-testid="asset-tags"
        />
        <p class="field-hint">
          Comma separated.
          <code class="text-fog-300">internet-facing</code> raises critical
          findings with a fix to P1.
        </p>
      </div>
      <p v-if="error" role="alert" class="text-sm text-[#ff8a99]">
        {{ error }}
      </p>
    </form>
    <template #footer>
      <button type="button" class="btn btn-ghost" @click="emit('close')">
        Cancel
      </button>
      <button
        type="submit"
        form="asset-form"
        class="btn btn-primary"
        :disabled="busy"
        data-testid="asset-save"
      >
        {{ busy ? "Saving…" : editing ? "Save changes" : "Add asset" }}
      </button>
    </template>
  </ModalDialog>
</template>
