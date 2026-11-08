<script setup lang="ts">
import AppIcon from './AppIcon.vue'
defineProps<{ message: string }>()
const emit = defineEmits<{ retry: [] }>()
</script>

<template>
  <div role="alert" class="flex flex-wrap items-start gap-3 rounded-md border border-p1/40 bg-p1/[0.06] px-4 py-3 text-sm" data-testid="error">
    <AppIcon name="circle-alert" :size="18" class="mt-0.5 text-[#ff8a99]" />
    <div class="min-w-0 flex-1">
      <p class="font-medium text-fog-100">Something went wrong</p>
      <p class="mt-0.5 break-words text-fog-300">{{ message }}</p>
    </div>
    <button type="button" class="btn btn-sm" @click="emit('retry')"><AppIcon name="refresh-cw" :size="13" />Retry</button>
  </div>
</template>
