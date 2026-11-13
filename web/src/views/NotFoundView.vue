<script setup lang="ts">
import EmptyState from "../components/EmptyState.vue";
</script>

<template>
  <div class="pt-10">
    <EmptyState icon="compass" title="Dead end">
      Nothing lives at this address. The page may have moved, or the link is
      wrong.
      <template #actions
        ><RouterLink to="/" class="btn btn-primary"
          >Back to overview</RouterLink
        ></template
      >
    </EmptyState>
  </div>
</template>
