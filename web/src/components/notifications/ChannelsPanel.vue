<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import AppIcon from '../AppIcon.vue'
import EmptyState from '../EmptyState.vue'
import LoadingBlock from '../LoadingBlock.vue'
import ErrorBlock from '../ErrorBlock.vue'
import ConfirmInline from '../ConfirmInline.vue'
import ModalDialog from '../ModalDialog.vue'
import { api, errorMessage } from '../../lib/api'
import { useAsync } from '../../lib/useAsync'
import { ago } from '../../lib/format'
import { SECRET_MASK, channelKindLabel } from '../../lib/labels'
import type { Channel, ChannelConfig, ChannelKind } from '../../lib/types'
import type { IconName } from '../../lib/icons'
import { useToasts } from '../../stores/toasts'

const toasts = useToasts()
const channels = useAsync(() => api.get<Channel[]>('/api/v1/channels'))
onMounted(() => channels.run())

const kindIcon: Record<ChannelKind, IconName> = { email: 'send', ntfy: 'bell', webhook: 'terminal' }

function target(c: Channel): string {
  if (c.kind === 'email') return c.config.to ?? ''
  if (c.kind === 'ntfy') return `${(c.config.server ?? 'https://ntfy.sh').replace(/\/$/, '')}/${c.config.topic ?? ''}`
  return c.config.url ?? ''
}

// --- form -------------------------------------------------------------------------
const formOpen = ref(false)
const editing = ref<Channel | null>(null)
const form = reactive({ name: '', kind: 'webhook' as ChannelKind, to: '', server: 'https://ntfy.sh', topic: '', token: '', url: '', secret: '' })
const busy = ref(false)
const formError = ref<string | null>(null)

function openNew() {
  editing.value = null
  Object.assign(form, { name: '', kind: 'webhook', to: '', server: 'https://ntfy.sh', topic: '', token: '', url: '', secret: '' })
  formError.value = null
  formOpen.value = true
}
function openEdit(c: Channel) {
  editing.value = c
  Object.assign(form, {
    name: c.name,
    kind: c.kind,
    to: c.config.to ?? '',
    server: c.config.server || 'https://ntfy.sh',
    topic: c.config.topic ?? '',
    token: c.config.token ?? '',
    url: c.config.url ?? '',
    secret: c.config.secret ?? '',
  })
  formError.value = null
  formOpen.value = true
}

// Secrets: on create an empty secret is omitted; on edit it is always sent, so "••••"
// keeps the stored value and "" clears it (per the contract).
function buildConfig(): ChannelConfig {
  const keepEmpty = !!editing.value
  if (form.kind === 'email') return { to: form.to.trim() }
  if (form.kind === 'ntfy') {
    const c: ChannelConfig = { server: form.server.trim() || 'https://ntfy.sh', topic: form.topic.trim() }
    if (form.token || keepEmpty) c.token = form.token
    return c
  }
  const c: ChannelConfig = { url: form.url.trim() }
  if (form.secret || keepEmpty) c.secret = form.secret
  return c
}

function validate(): string | null {
  if (!form.name.trim()) return 'A name is required.'
  if (form.kind === 'email' && !/^[^\s@]+@[^\s@]+$/.test(form.to.trim().split(',')[0] ?? '')) return 'Enter an email address.'
  if (form.kind === 'ntfy') {
    if (!/^https?:\/\//.test(form.server.trim() || 'https://ntfy.sh')) return 'The ntfy server must be an http(s) URL.'
    if (!form.topic.trim()) return 'A topic is required.'
  }
  if (form.kind === 'webhook' && !/^https?:\/\/\S+$/.test(form.url.trim())) return 'Enter an http(s) URL.'
  return null
}

async function save() {
  formError.value = validate()
  if (formError.value) return
  busy.value = true
  try {
    const body = { name: form.name.trim(), kind: form.kind, config: buildConfig() }
    if (editing.value) await api.patch(`/api/v1/channels/${editing.value.id}`, body)
    else await api.post('/api/v1/channels', body)
    toasts.ok(editing.value ? 'Channel saved.' : 'Channel created. Send a test to check it.')
    formOpen.value = false
    channels.run()
  } catch (e) {
    formError.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

const secretKept = computed(() => ({
  token: !!editing.value && form.token === SECRET_MASK,
  secret: !!editing.value && form.secret === SECRET_MASK,
}))

// --- test / delete ------------------------------------------------------------------
const testState = ref<Record<number, { busy: boolean; ok?: boolean; message?: string }>>({})
async function test(c: Channel) {
  testState.value = { ...testState.value, [c.id]: { busy: true } }
  try {
    await api.post(`/api/v1/channels/${c.id}/test`)
    testState.value = { ...testState.value, [c.id]: { busy: false, ok: true, message: 'Test sent' } }
  } catch (e) {
    testState.value = { ...testState.value, [c.id]: { busy: false, ok: false, message: errorMessage(e) } }
  }
}

const busyId = ref<number | null>(null)
async function remove(c: Channel) {
  busyId.value = c.id
  try {
    await api.del(`/api/v1/channels/${c.id}`)
    toasts.ok(`Deleted channel ${c.name}.`)
    channels.run()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}
</script>

<template>
  <div>
    <div class="mb-4 flex items-center justify-between gap-3">
      <p class="text-sm text-fog-400">Where alerts are delivered.</p>
      <button type="button" class="btn btn-primary" data-testid="channel-new" @click="openNew"><AppIcon name="plus" :size="14" />New channel</button>
    </div>

    <LoadingBlock v-if="channels.loading.value && !channels.data.value" :rows="3" label="Loading channels" />
    <ErrorBlock v-else-if="channels.error.value" :message="channels.error.value" @retry="channels.run()" />
    <template v-else-if="channels.data.value">
      <EmptyState v-if="!channels.data.value.length" icon="bell" title="No channels yet">
        Add an email address, an ntfy topic or a webhook. Then create rules that decide which events reach it.
        <template #actions><button type="button" class="btn btn-primary" @click="openNew"><AppIcon name="plus" :size="14" />Add a channel</button></template>
      </EmptyState>
      <ul v-else class="divide-y divide-ink-700 border-y border-ink-700">
        <li v-for="c in channels.data.value" :key="c.id" class="flex flex-wrap items-center gap-x-4 gap-y-2 py-3" data-testid="channel-row" :data-name="c.name">
          <span class="grid h-8 w-8 shrink-0 place-items-center rounded-[4px] border border-ink-600 bg-ink-850 text-fog-300">
            <AppIcon :name="kindIcon[c.kind]" :size="14" />
          </span>
          <div class="min-w-0 flex-1">
            <p class="flex items-center gap-2">
              <span class="font-medium text-fog-100">{{ c.name }}</span>
              <span class="tag">{{ channelKindLabel[c.kind] }}</span>
            </p>
            <p class="truncate font-mono text-xs text-fog-400" :title="target(c)">{{ target(c) }}</p>
          </div>
          <p
            v-if="testState[c.id]?.message"
            class="flex max-w-xs items-center gap-1.5 text-xs"
            :class="testState[c.id].ok ? 'text-ok' : 'text-[#ff8a99]'"
            role="status"
            data-testid="channel-test-result"
            :data-ok="testState[c.id].ok ? 'true' : 'false'"
          >
            <AppIcon :name="testState[c.id].ok ? 'circle-check' : 'circle-x'" :size="13" />
            <span class="truncate" :title="testState[c.id].message">{{ testState[c.id].message }}</span>
          </p>
          <span class="text-xs text-fog-500">added {{ ago(c.createdAt) }}</span>
          <ConfirmInline :question="`Delete ${c.name}? Rules using it stop sending.`" :busy="busyId === c.id" @confirm="remove(c)">
            <template #default="{ ask }">
              <button type="button" class="btn btn-sm" :disabled="testState[c.id]?.busy" data-testid="channel-test" @click="test(c)">
                <AppIcon :name="testState[c.id]?.busy ? 'loader-circle' : 'send'" :size="13" :class="testState[c.id]?.busy ? 'animate-spin' : ''" />Send test
              </button>
              <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Edit ${c.name}`" data-testid="channel-edit" @click="openEdit(c)"><AppIcon name="pencil" :size="13" /></button>
              <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Delete ${c.name}`" data-testid="channel-delete" @click="ask"><AppIcon name="trash-2" :size="13" /></button>
            </template>
          </ConfirmInline>
        </li>
      </ul>
    </template>

    <ModalDialog :open="formOpen" :title="editing ? 'Edit channel' : 'New channel'" testid="channel-dialog" @close="formOpen = false">
      <form id="channel-form" class="space-y-4" @submit.prevent="save">
        <div class="grid gap-4 sm:grid-cols-[1fr_9rem]">
          <div>
            <label for="channel-name" class="field-label">Name</label>
            <input id="channel-name" v-model="form.name" class="input" placeholder="e.g. security-team" required data-testid="channel-name" data-autofocus />
          </div>
          <div>
            <label for="channel-kind" class="field-label">Kind</label>
            <div class="select-wrap">
              <select id="channel-kind" v-model="form.kind" class="select" :disabled="!!editing" data-testid="channel-kind">
                <option value="email">Email</option>
                <option value="ntfy">ntfy</option>
                <option value="webhook">Webhook</option>
              </select>
            </div>
          </div>
        </div>

        <template v-if="form.kind === 'email'">
          <div>
            <label for="channel-to" class="field-label">To</label>
            <input id="channel-to" v-model="form.to" type="email" class="input" placeholder="security@example.com" required data-testid="channel-to" />
          </div>
        </template>

        <template v-else-if="form.kind === 'ntfy'">
          <div>
            <label for="channel-server" class="field-label">Server</label>
            <input id="channel-server" v-model="form.server" class="input font-mono" placeholder="https://ntfy.sh" data-testid="channel-server" />
          </div>
          <div>
            <label for="channel-topic" class="field-label">Topic</label>
            <input id="channel-topic" v-model="form.topic" class="input font-mono" placeholder="acme-security-alerts" required data-testid="channel-topic" />
            <p class="field-hint">On the public server anyone who knows the topic can read it. Pick something hard to guess.</p>
          </div>
          <div>
            <label for="channel-token" class="field-label">Access token <span class="text-fog-500">(optional)</span></label>
            <input id="channel-token" v-model="form.token" type="password" autocomplete="new-password" class="input font-mono" data-testid="channel-token" />
            <p v-if="secretKept.token" class="field-hint">A token is stored. Leave as is to keep it, or clear the field to remove it.</p>
          </div>
        </template>

        <template v-else>
          <div>
            <label for="channel-url" class="field-label">URL</label>
            <input id="channel-url" v-model="form.url" type="url" class="input font-mono" placeholder="https://hooks.example.com/vengeance" required data-testid="channel-url" />
            <p class="field-hint">Receives a JSON POST per notification.</p>
          </div>
          <div>
            <label for="channel-secret" class="field-label">Signing secret <span class="text-fog-500">(optional)</span></label>
            <input id="channel-secret" v-model="form.secret" type="password" autocomplete="new-password" class="input font-mono" data-testid="channel-secret" />
            <p v-if="secretKept.secret" class="field-hint">A secret is stored. Leave as is to keep it, or clear the field to remove it.</p>
            <p v-else class="field-hint">Used to sign each request so the receiver can verify it came from Vengeance.</p>
          </div>
        </template>

        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">{{ formError }}</p>
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost" @click="formOpen = false">Cancel</button>
        <button type="submit" form="channel-form" class="btn btn-primary" :disabled="busy" data-testid="channel-save">
          {{ busy ? 'Saving…' : editing ? 'Save' : 'Create channel' }}
        </button>
      </template>
    </ModalDialog>
  </div>
</template>
