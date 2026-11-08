<script setup lang="ts">
import { onMounted } from 'vue'
import AppIcon from '../AppIcon.vue'
import EmptyState from '../EmptyState.vue'
import LoadingBlock from '../LoadingBlock.vue'
import ErrorBlock from '../ErrorBlock.vue'
import { api } from '../../lib/api'
import { useAsync } from '../../lib/useAsync'
import { ago, dateTime } from '../../lib/format'
import type { NotificationLog } from '../../lib/types'

const log = useAsync(() => api.get<NotificationLog[]>('/api/v1/notifications', { limit: 50 }))
onMounted(() => log.run())

const statusClass: Record<NotificationLog['status'], string> = {
  sent: 'text-ok border-ok/40',
  failed: 'text-[#ff8a99] border-p1/50 bg-p1/10',
  pending: 'text-fog-300 border-ink-600',
  digest: 'text-beam border-beam/40',
}
const statusLabel: Record<NotificationLog['status'], string> = {
  sent: 'Sent',
  failed: 'Failed',
  pending: 'Pending',
  digest: 'In digest',
}
</script>

<template>
  <div>
    <div class="mb-4 flex items-center justify-between gap-3">
      <p class="text-sm text-fog-400">The last 50 notifications.</p>
      <button type="button" class="btn btn-ghost" :disabled="log.loading.value" @click="log.run()">
        <AppIcon name="refresh-cw" :size="14" :class="log.loading.value ? 'animate-spin' : ''" />Refresh
      </button>
    </div>
    <LoadingBlock v-if="log.loading.value && !log.data.value" :rows="5" label="Loading log" />
    <ErrorBlock v-else-if="log.error.value" :message="log.error.value" @retry="log.run()" />
    <template v-else-if="log.data.value">
      <EmptyState v-if="!log.data.value.length" icon="clock" title="Nothing sent yet">
        Once a rule matches an event, each notification is logged here with its delivery status. Use "Send test" on a channel to check delivery now.
      </EmptyState>
      <div v-else class="-mx-4 overflow-x-auto sm:mx-0">
        <table class="dtable min-w-[48rem]">
          <thead>
            <tr>
              <th>When</th>
              <th>Subject</th>
              <th>Rule</th>
              <th>Channel</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="n in log.data.value" :key="n.id" data-testid="log-row" :data-status="n.status">
              <td class="num text-xs whitespace-nowrap text-fog-300" :title="dateTime(n.createdAt)">{{ ago(n.createdAt) }}</td>
              <td class="max-w-[24rem]">
                <p class="truncate text-fog-100" :title="n.subject">{{ n.subject }}</p>
                <p v-if="n.error" class="truncate text-xs text-[#ff8a99]" :title="n.error">{{ n.error }}</p>
              </td>
              <td class="text-fog-300">{{ n.ruleName }}</td>
              <td class="text-fog-300">{{ n.channelName }}</td>
              <td class="whitespace-nowrap">
                <span class="inline-flex h-5 items-center rounded-[3px] border px-1.5 text-[0.7rem] font-medium" :class="statusClass[n.status]">{{ statusLabel[n.status] ?? n.status }}</span>
                <span v-if="n.sentAt" class="num ml-2 text-[0.7rem] text-fog-500" :title="dateTime(n.sentAt)">{{ ago(n.sentAt) }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>
