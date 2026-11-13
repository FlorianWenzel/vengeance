<script setup lang="ts">
import { computed } from "vue";
import { icons, type IconName } from "../lib/icons";

const props = withDefaults(
  defineProps<{ name: IconName; size?: number | string; label?: string }>(),
  { size: 16 },
);
const body = computed(() => icons[props.name]);
</script>

<template>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    :width="size"
    :height="size"
    class="inline-block shrink-0"
    :role="label ? 'img' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : 'true'"
    focusable="false"
    v-html="body"
  />
</template>
