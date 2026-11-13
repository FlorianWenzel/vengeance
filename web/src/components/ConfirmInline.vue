<script setup lang="ts">
// In-page confirmation (replaces window.confirm): shows a short question with
// Cancel / Confirm buttons in place of the trigger.
import { nextTick, ref } from "vue";

withDefaults(
  defineProps<{
    question: string;
    confirmLabel?: string;
    busy?: boolean;
    testid?: string;
  }>(),
  { confirmLabel: "Delete", busy: false },
);
const emit = defineEmits<{ confirm: [] }>();
const asking = ref(false);
const cancelBtn = ref<HTMLButtonElement | null>(null);

function ask() {
  asking.value = true;
  nextTick(() => cancelBtn.value?.focus());
}
function cancel() {
  asking.value = false;
}
defineExpose({ cancel });
</script>

<template>
  <span class="inline-flex items-center gap-2">
    <slot v-if="!asking" :ask="ask" />
    <span
      v-else
      role="alertdialog"
      aria-live="assertive"
      class="inline-flex flex-wrap items-center gap-2 rounded-[4px] border border-p1/40 bg-p1/[0.07] py-1 pr-1 pl-2.5 text-xs text-fog-100"
      @keydown.esc.stop.prevent="cancel"
    >
      <span>{{ question }}</span>
      <button
        ref="cancelBtn"
        type="button"
        class="btn btn-sm btn-ghost"
        @click="cancel"
      >
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-sm btn-danger"
        :disabled="busy"
        :data-testid="testid"
        @click="emit('confirm')"
      >
        {{ busy ? "Working…" : confirmLabel }}
      </button>
    </span>
  </span>
</template>
