<script setup lang="ts">
import CopyButton from "./CopyButton.vue";
defineProps<{ command: string; testid?: string }>();
</script>

<template>
  <div class="flex items-stretch gap-2">
    <code class="codeblock min-w-0 flex-1" :data-testid="testid">{{
      command
    }}</code>
    <CopyButton :text="command" class="self-center" />
  </div>
</template>
