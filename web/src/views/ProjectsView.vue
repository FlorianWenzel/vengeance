<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
import AppIcon from '../components/AppIcon.vue'
import PageHeader from '../components/PageHeader.vue'
import EmptyState from '../components/EmptyState.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import ConfirmInline from '../components/ConfirmInline.vue'
import { api, errorMessage } from '../lib/api'
import type { Project } from '../lib/types'
import { useProjects } from '../stores/projects'
import { useToasts } from '../stores/toasts'

const projects = useProjects()
const toasts = useToasts()
onMounted(() => projects.refresh())

const newName = ref('')
const creating = ref(false)
const newInput = ref<HTMLInputElement | null>(null)

async function create() {
  const name = newName.value.trim()
  if (!name) return
  creating.value = true
  try {
    await api.post<Project>('/api/v1/projects', { name })
    newName.value = ''
    toasts.ok(`Project ${name} created.`)
    await projects.refresh()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    creating.value = false
  }
}

const editId = ref<number | null>(null)
const editName = ref('')
const busyId = ref<number | null>(null)

function startEdit(p: Project) {
  editId.value = p.id
  editName.value = p.name
  nextTick(() => (document.getElementById(`project-edit-${p.id}`) as HTMLInputElement | null)?.select())
}
async function saveEdit(p: Project) {
  const name = editName.value.trim()
  if (!name || name === p.name) {
    editId.value = null
    return
  }
  busyId.value = p.id
  try {
    await api.patch(`/api/v1/projects/${p.id}`, { name })
    editId.value = null
    await projects.refresh()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}
async function remove(p: Project) {
  busyId.value = p.id
  try {
    await api.del(`/api/v1/projects/${p.id}`)
    toasts.ok(`Deleted project ${p.name}. Its assets are now unassigned.`)
    await projects.refresh()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}
</script>

<template>
  <div class="max-w-3xl">
    <PageHeader title="Projects" eyebrow="Inventory">
      Group assets by product or team. Projects drive filters, per-project coverage scores and notification rules.
    </PageHeader>

    <form class="mb-6 flex flex-wrap items-end gap-2" @submit.prevent="create">
      <div class="min-w-0 flex-1">
        <label for="project-name" class="field-label">New project</label>
        <input id="project-name" ref="newInput" v-model="newName" class="input" placeholder="e.g. payments" data-testid="project-name" />
      </div>
      <button type="submit" class="btn btn-primary" :disabled="creating || !newName.trim()" data-testid="project-save">
        <AppIcon name="plus" :size="14" />Create
      </button>
    </form>

    <LoadingBlock v-if="projects.loading && !projects.loaded" :rows="4" label="Loading projects" />
    <ErrorBlock v-else-if="projects.error" :message="projects.error" @retry="projects.refresh()" />
    <EmptyState v-else-if="!projects.list.length" icon="folder" title="No projects yet">
      Create one above, or pass <code class="font-mono text-xs text-fog-200">project=name</code> when ingesting and it is created for you.
    </EmptyState>
    <table v-else class="dtable">
      <thead>
        <tr>
          <th>Name</th>
          <th class="text-right">Assets</th>
          <th class="text-right">Open findings</th>
          <th class="w-px"><span class="sr-only">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="p in projects.list" :key="p.id" data-testid="project-row">
          <td>
            <form v-if="editId === p.id" class="flex items-center gap-2" @submit.prevent="saveEdit(p)">
              <label :for="`project-edit-${p.id}`" class="sr-only">Project name</label>
              <input :id="`project-edit-${p.id}`" v-model="editName" class="input !h-7 max-w-xs" @keydown.esc.prevent="editId = null" />
              <button type="submit" class="btn btn-sm btn-primary" :disabled="busyId === p.id">Save</button>
              <button type="button" class="btn btn-sm btn-ghost" @click="editId = null">Cancel</button>
            </form>
            <span v-else class="font-medium text-fog-100">{{ p.name }}</span>
          </td>
          <td class="num text-right">
            <RouterLink :to="{ path: '/assets', query: { project: String(p.id) } }" class="hover:text-beam">{{ p.assets }}</RouterLink>
          </td>
          <td class="num text-right">
            <RouterLink :to="{ path: '/findings', query: { project: String(p.id) } }" class="hover:text-beam">{{ p.open }}</RouterLink>
          </td>
          <td>
            <div class="flex justify-end">
              <ConfirmInline :question="`Delete ${p.name}? Assets stay, unassigned.`" :busy="busyId === p.id" @confirm="remove(p)">
                <template #default="{ ask }">
                  <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Rename ${p.name}`" @click="startEdit(p)"><AppIcon name="pencil" :size="13" /></button>
                  <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Delete ${p.name}`" @click="ask"><AppIcon name="trash-2" :size="13" /></button>
                </template>
              </ConfirmInline>
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
