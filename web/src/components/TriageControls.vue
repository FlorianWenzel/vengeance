<script setup lang="ts">
import { computed, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import ModalDialog from './ModalDialog.vue'
import { api, errorMessage } from '../lib/api'
import { isoDay, plural } from '../lib/format'
import type { State, TriageRequest } from '../lib/types'
import { useToasts } from '../stores/toasts'

const props = withDefaults(
  defineProps<{
    ids: number[]
    /** Current state for single-finding mode; omitted in bulk mode (all actions offered). */
    state?: State
    testPrefix?: string
    size?: 'sm' | 'md'
  }>(),
  { testPrefix: 'bulk', size: 'sm' },
)
const emit = defineEmits<{ done: [updated: number] }>()
const toasts = useToasts()

const busy = ref(false)
const dialog = ref<null | 'accept' | 'fp'>(null)
const until = ref('')
const reason = ref('')
const formError = ref<string | null>(null)

const can = computed(() => {
  const s = props.state
  if (!s) return { ack: true, accept: true, fp: true, reopen: true }
  return {
    ack: s === 'new',
    accept: s === 'new' || s === 'acknowledged' || s === 'accepted',
    fp: s === 'new' || s === 'acknowledged',
    reopen: s === 'acknowledged' || s === 'accepted' || s === 'false_positive',
  }
})

const minDate = isoDay(1)
const presets = [
  { label: '30 days', days: 30 },
  { label: '90 days', days: 90 },
  { label: '1 year', days: 365 },
]

const acceptValid = computed(() => /^\d{4}-\d{2}-\d{2}$/.test(until.value) && until.value >= minDate)
const fpValid = computed(() => reason.value.trim().length > 0)
const count = computed(() => props.ids.length)
const noun = computed(() => (count.value === 1 ? 'this finding' : `${count.value} findings`))

function open(kind: 'accept' | 'fp') {
  formError.value = null
  reason.value = ''
  until.value = ''
  dialog.value = kind
}

async function send(body: Omit<TriageRequest, 'ids'>, verb: string) {
  if (!props.ids.length || busy.value) return
  busy.value = true
  formError.value = null
  try {
    const res = await api.post<{ updated: number }>('/api/v1/findings/triage', { ids: props.ids, ...body })
    const n = res?.updated ?? props.ids.length
    toasts.ok(`${verb} ${plural(n, 'finding')}.`)
    dialog.value = null
    emit('done', n)
  } catch (e) {
    if (dialog.value) formError.value = errorMessage(e)
    else toasts.error(errorMessage(e))
  } finally {
    busy.value = false
  }
}

function confirm() {
  if (dialog.value === 'accept') {
    if (!acceptValid.value) {
      formError.value = 'Pick a date in the future.'
      return
    }
    send({ state: 'accepted', until: until.value, reason: reason.value.trim() || undefined }, 'Accepted risk for')
  } else if (dialog.value === 'fp') {
    if (!fpValid.value) {
      formError.value = 'A reason is required.'
      return
    }
    send({ state: 'false_positive', reason: reason.value.trim() }, 'Marked as false positive:')
  }
}

const btn = computed(() => (props.size === 'sm' ? 'btn btn-sm' : 'btn'))
</script>

<template>
  <div class="flex flex-wrap items-center gap-1.5">
    <button v-if="can.ack" type="button" :class="btn" :disabled="busy" :data-testid="`${testPrefix}-acknowledge`" @click="send({ state: 'acknowledged' }, 'Acknowledged')">
      <AppIcon name="eye" :size="13" />Acknowledge
    </button>
    <button v-if="can.accept" type="button" :class="btn" :disabled="busy" :data-testid="`${testPrefix}-accept`" @click="open('accept')">
      <AppIcon name="shield-check" :size="13" />Accept risk
    </button>
    <button v-if="can.fp" type="button" :class="btn" :disabled="busy" :data-testid="`${testPrefix}-false-positive`" @click="open('fp')">
      <AppIcon name="ban" :size="13" />False positive
    </button>
    <button v-if="can.reopen" type="button" :class="btn" :disabled="busy" :data-testid="`${testPrefix}-reopen`" @click="send({ state: 'new' }, 'Reopened')">
      <AppIcon name="rotate-ccw" :size="13" />Reopen
    </button>

    <ModalDialog :open="dialog === 'accept'" title="Accept risk" testid="accept-dialog" @close="dialog = null">
      <form id="accept-form" class="space-y-4" @submit.prevent="confirm">
        <p class="text-sm text-fog-300">
          Hide {{ noun }} from the inbox until a date. When it passes, the finding comes back and an
          <em>Accepted risk expired</em> notification can fire.
        </p>
        <div>
          <label for="accept-until" class="field-label">Accepted until <span class="text-p1">*</span></label>
          <input id="accept-until" v-model="until" type="date" class="input num w-48" :min="minDate" required data-testid="accept-until" data-autofocus />
          <div class="mt-2 flex flex-wrap gap-1.5">
            <button v-for="p in presets" :key="p.days" type="button" class="btn btn-sm btn-ghost !border-ink-600" @click="until = isoDay(p.days)">+{{ p.label }}</button>
          </div>
        </div>
        <div>
          <label for="accept-note" class="field-label">Note <span class="text-fog-500">(optional)</span></label>
          <textarea id="accept-note" v-model="reason" rows="2" class="input" placeholder="Why is this acceptable for now?" />
        </div>
        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">{{ formError }}</p>
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost" @click="dialog = null">Cancel</button>
        <button type="submit" form="accept-form" class="btn btn-primary" :disabled="busy || !acceptValid" data-testid="dialog-confirm">
          {{ busy ? 'Saving…' : 'Accept risk' }}
        </button>
      </template>
    </ModalDialog>

    <ModalDialog :open="dialog === 'fp'" title="Mark as false positive" testid="fp-dialog" @close="dialog = null">
      <form id="fp-form" class="space-y-4" @submit.prevent="confirm">
        <p class="text-sm text-fog-300">Close {{ noun }} for good. It will not come back while scans keep reporting it.</p>
        <div>
          <label for="fp-reason" class="field-label">Reason <span class="text-p1">*</span></label>
          <textarea id="fp-reason" v-model="reason" rows="3" class="input" required placeholder="e.g. the vulnerable code path is not reachable" data-testid="fp-reason" data-autofocus />
        </div>
        <p v-if="formError" role="alert" class="text-sm text-[#ff8a99]">{{ formError }}</p>
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost" @click="dialog = null">Cancel</button>
        <button type="submit" form="fp-form" class="btn btn-primary" :disabled="busy || !fpValid" data-testid="dialog-confirm">
          {{ busy ? 'Saving…' : 'Mark false positive' }}
        </button>
      </template>
    </ModalDialog>
  </div>
</template>
