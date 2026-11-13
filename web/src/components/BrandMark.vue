<script setup lang="ts">
// Original mark: two searchlight beams converging into a "V" inside a pointed lens.
withDefaults(defineProps<{ size?: number }>(), { size: 28 });
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 32 32"
    aria-hidden="true"
    focusable="false"
  >
    <path
      d="M16 2.5C24 8 29.5 12 29.5 16S24 24 16 29.5C8 24 2.5 20 2.5 16S8 8 16 2.5Z"
      fill="none"
      stroke="currentColor"
      stroke-opacity=".28"
      stroke-width="1.2"
    />
    <path
      d="M6.5 7.5 16 27 25.5 7.5h-3.9L16 19.4 10.4 7.5Z"
      fill="var(--color-beam)"
    />
    <path
      d="M12.6 7.5h6.8L16 14.6Z"
      fill="var(--color-beam)"
      fill-opacity=".45"
    />
  </svg>
</template>
