<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import AppIcon from '../components/AppIcon.vue'
import PageHeader from '../components/PageHeader.vue'
import EmptyState from '../components/EmptyState.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import ConfirmInline from '../components/ConfirmInline.vue'
import ModalDialog from '../components/ModalDialog.vue'
import CopyButton from '../components/CopyButton.vue'
import CommandBlock from '../components/CommandBlock.vue'
import { api, errorMessage } from '../lib/api'
import { useAsync } from '../lib/useAsync'
import { ago, dateTime } from '../lib/format'
import type { ApiToken } from '../lib/types'
import { useToasts } from '../stores/toasts'

const toasts = useToasts()
const tokens = useAsync(() => api.get<ApiToken[]>('/api/v1/tokens'))
onMounted(() => tokens.run())

const endpoint = `${location.origin}/mcp`

// --- create -----------------------------------------------------------------
const formOpen = ref(false)
const form = reactive({ name: '', scope: 'read' as 'read' | 'write' })
const formBusy = ref(false)
const formError = ref<string | null>(null)

function openNew() {
  form.name = ''
  form.scope = 'read'
  formError.value = null
  formOpen.value = true
}

async function save() {
  formError.value = null
  if (!form.name.trim()) {
    formError.value = 'A name is required, for example "Claude Code on my laptop".'
    return
  }
  formBusy.value = true
  try {
    const t = await api.post<ApiToken & { token: string }>('/api/v1/tokens', { name: form.name.trim(), scope: form.scope })
    formOpen.value = false
    created.value = { name: t.name, scope: t.scope, value: t.token }
    tokens.run()
  } catch (e) {
    formError.value = errorMessage(e)
  } finally {
    formBusy.value = false
  }
}

// --- reveal -----------------------------------------------------------------
const created = ref<{ name: string; scope: string; value: string } | null>(null)
const tok = computed(() => created.value?.value ?? '<token>')
const claudeCode = computed(() => `claude mcp add --transport http vengeance ${endpoint} \\\n  --header "Authorization: Bearer ${tok.value}"`)
const jsonConfig = computed(() =>
  JSON.stringify({ mcpServers: { vengeance: { type: 'http', url: endpoint, headers: { Authorization: `Bearer ${tok.value}` } } } }, null, 2),
)

// --- revoke -----------------------------------------------------------------
const busyId = ref<number | null>(null)
async function revoke(t: ApiToken) {
  busyId.value = t.id
  try {
    await api.del(`/api/v1/tokens/${t.id}`)
    toasts.ok(`Revoked ${t.name}. Assistants using it are disconnected.`)
    tokens.run()
  } catch (e) {
    toasts.error(errorMessage(e))
  } finally {
    busyId.value = null
  }
}

const prompts = [
  'What should I patch first this week, and how?',
  'Compare my GitHub repositories with the Vengeance inventory and add the missing ones.',
  'Which images running in the cluster have known-exploited vulnerabilities?',
  'Which assets have not been scanned in the last week?',
]
</script>

<template>
  <div>
    <PageHeader title="AI assistants" eyebrow="Setup">
      Connect Claude or any other MCP client. It can read findings, fixes, inventory and coverage, and with a read-write token
      triage findings and add assets. Each assistant gets its own token, so you can revoke one without touching the others.
      <template #actions>
        <button type="button" class="btn btn-primary" data-testid="ai-token-new" @click="openNew"><AppIcon name="plus" :size="14" />New token</button>
      </template>
    </PageHeader>

    <section class="mb-8 grid gap-6 lg:grid-cols-2">
      <div class="min-w-0">
        <p class="eyebrow mb-2">MCP endpoint</p>
        <div class="flex items-stretch gap-2">
          <code class="codeblock min-w-0 flex-1 select-all" data-testid="ai-endpoint">{{ endpoint }}</code>
          <CopyButton :text="endpoint" class="self-center" />
        </div>
        <p class="field-hint mt-2">Streamable HTTP. Authenticate with <code>Authorization: Bearer vgp_…</code>.</p>
      </div>
      <div class="min-w-0">
        <p class="eyebrow mb-2">Things to ask</p>
        <ul class="space-y-1.5 text-sm text-fog-300">
          <li v-for="p in prompts" :key="p" class="flex gap-2"><AppIcon name="bot" :size="14" class="mt-0.5 shrink-0 text-beam" />{{ p }}</li>
        </ul>
      </div>
    </section>

    <LoadingBlock v-if="tokens.loading.value && !tokens.data.value" :rows="3" label="Loading tokens" />
    <ErrorBlock v-else-if="tokens.error.value" :message="tokens.error.value" @retry="tokens.run()" />
    <template v-else-if="tokens.data.value">
      <EmptyState v-if="!tokens.data.value.length" icon="bot" title="No assistants connected">
        Create a token, then add Vengeance to Claude Code, Claude Desktop or another MCP client with the snippet shown.
        <template #actions>
          <button type="button" class="btn btn-primary" @click="openNew"><AppIcon name="plus" :size="14" />Create a token</button>
        </template>
      </EmptyState>
      <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
        <table class="dtable min-w-[46rem]">
          <thead>
            <tr>
              <th>Name</th>
              <th>Access</th>
              <th>Token</th>
              <th>Last used</th>
              <th class="w-px"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="t in tokens.data.value" :key="t.id" data-testid="ai-token-row" :data-name="t.name">
              <td>
                <span class="font-medium text-fog-100">{{ t.name }}</span>
                <p class="text-[0.7rem] text-fog-500">created {{ ago(t.createdAt) }}<template v-if="t.createdBy"> by {{ t.createdBy }}</template></p>
              </td>
              <td>
                <span
                  class="inline-flex h-5 items-center rounded-[3px] border px-1.5 text-[0.68rem] font-medium"
                  :class="t.scope === 'write' ? 'border-beam/50 bg-beam/10 text-beam' : 'border-fog-500/40 text-fog-300'"
                >{{ t.scope === 'write' ? 'read and write' : 'read only' }}</span>
              </td>
              <td><code class="font-mono text-xs text-fog-300">{{ t.tokenPrefix }}…</code></td>
              <td class="num whitespace-nowrap text-fog-200" :title="dateTime(t.lastUsedAt)">{{ t.lastUsedAt ? ago(t.lastUsedAt) : 'never' }}</td>
              <td>
                <div class="flex justify-end">
                  <ConfirmInline :question="`Revoke ${t.name}? The assistant loses access now.`" confirm-label="Revoke" :busy="busyId === t.id" testid="ai-token-revoke-confirm" @confirm="revoke(t)">
                    <template #default="{ ask }">
                      <button type="button" class="btn btn-ghost btn-sm" :aria-label="`Revoke ${t.name}`" data-testid="ai-token-revoke" @click="ask">
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

    <ModalDialog :open="formOpen" title="New assistant token" testid="ai-token-dialog" @close="formOpen = false">
      <form id="ai-token-form" class="space-y-4" @submit.prevent="save">
        <div>
          <label for="ai-token-name" class="field-label">Name</label>
          <input id="ai-token-name" v-model="form.name" class="input" placeholder="e.g. Claude Code on my laptop" data-testid="ai-token-name" data-autofocus />
        </div>
        <fieldset>
          <legend class="field-label">Access</legend>
          <div class="grid gap-2 sm:grid-cols-2">
            <label class="flex cursor-pointer gap-2 rounded-[4px] border border-fog-500/30 p-3 text-sm has-[:checked]:border-beam/60 has-[:checked]:bg-beam/[0.06]">
              <input v-model="form.scope" type="radio" value="read" name="scope" data-testid="ai-token-scope-read" />
              <span><span class="block font-medium text-fog-100">Read only</span><span class="text-fog-400">Ask questions, nothing changes.</span></span>
            </label>
            <label class="flex cursor-pointer gap-2 rounded-[4px] border border-fog-500/30 p-3 text-sm has-[:checked]:border-beam/60 has-[:checked]:bg-beam/[0.06]">
              <input v-model="form.scope" type="radio" value="write" name="scope" data-testid="ai-token-scope-write" />
              <span><span class="block font-medium text-fog-100">Read and write</span><span class="text-fog-400">Also triage findings and edit the inventory.</span></span>
            </label>
          </div>
        </fieldset>
        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">{{ formError }}</p>
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost" @click="formOpen = false">Cancel</button>
        <button type="submit" form="ai-token-form" class="btn btn-primary" :disabled="formBusy" data-testid="ai-token-save">{{ formBusy ? 'Creating…' : 'Create token' }}</button>
      </template>
    </ModalDialog>

    <ModalDialog :open="!!created" title="Connect your assistant" width="42rem" testid="ai-token-created" @close="created = null">
      <template v-if="created">
        <p class="flex items-start gap-2 rounded-[4px] border border-beam/40 bg-beam/[0.07] px-3 py-2 text-sm text-fog-100">
          <AppIcon name="key-round" :size="15" class="mt-0.5 text-beam" />
          <span>This is the only time the token for <strong>{{ created.name }}</strong> is shown.</span>
        </p>
        <label class="field-label mt-4" for="ai-token-value">Token</label>
        <div class="flex items-stretch gap-2">
          <code id="ai-token-value" class="codeblock min-w-0 flex-1 !border-l-beam select-all" data-testid="ai-token-value">{{ created.value }}</code>
          <CopyButton :text="created.value" class="self-center" data-autofocus />
        </div>
        <p class="eyebrow mt-6 mb-2">Claude Code</p>
        <CommandBlock :command="claudeCode" testid="ai-claude-code" />
        <p class="eyebrow mt-5 mb-2">Other MCP clients</p>
        <CommandBlock :command="jsonConfig" />
      </template>
      <template #footer>
        <button type="button" class="btn btn-primary" @click="created = null">Done</button>
      </template>
    </ModalDialog>
  </div>
</template>
