<script setup lang="ts">
defineProps<{ title: string; eyebrow?: string }>();
</script>

<template>
  <header class="mb-6 flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
    <div class="min-w-0">
      <p v-if="eyebrow" class="eyebrow mb-1">{{ eyebrow }}</p>
      <h1 class="text-[2rem] leading-none text-fog-100 sm:text-[2.4rem]">
        {{ title }}
      </h1>
      <p v-if="$slots.default" class="mt-2 max-w-2xl text-sm text-fog-400">
        <slot />
      </p>
    </div>
    <div v-if="$slots.actions" class="flex flex-wrap items-center gap-2">
      <slot name="actions" />
    </div>
  </header>
</template>
