<script setup lang="ts">
import PageHeader from '../components/PageHeader.vue'
import FindingsInbox from '../components/FindingsInbox.vue'
</script>

<template>
  <div>
    <PageHeader title="Findings" eyebrow="Inbox">
      Every scanner result, deduplicated. Triage what matters; fixed findings close themselves when the next scan no longer reports them.
    </PageHeader>
    <FindingsInbox sync-url />
  </div>
</template>
