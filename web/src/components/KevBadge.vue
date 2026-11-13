<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
withDefaults(defineProps<{ compact?: boolean }>(), { compact: false });
</script>

<template>
  <span
    class="inline-flex h-5 items-center gap-1 rounded-[3px] border border-p1/60 bg-p1/10 px-1.5 font-mono text-[0.68rem] font-bold tracking-wide text-[#ff8a99]"
    title="Listed in the CISA Known Exploited Vulnerabilities catalog"
  >
    <AppIcon name="flame" :size="11" />
    <span>KEV</span
    ><span v-if="!compact" class="font-sans font-normal tracking-normal"
      >exploited</span
    >
  </span>
</template>
