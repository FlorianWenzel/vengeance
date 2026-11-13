<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
import { useToasts } from "../stores/toasts";
const toasts = useToasts();
</script>

<template>
  <div
    class="pointer-events-none fixed right-4 bottom-4 z-[70] flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-2"
    aria-live="polite"
    role="status"
  >
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts.items"
        :key="t.id"
        class="pointer-events-auto flex items-start gap-2.5 rounded-md border bg-ink-800 px-3.5 py-2.5 text-sm shadow-[0_12px_40px_-12px_rgba(0,0,0,.8)]"
        :class="
          t.kind === 'error'
            ? 'border-p1/50'
            : t.kind === 'ok'
              ? 'border-ok/40'
              : 'border-ink-600'
        "
      >
        <AppIcon
          :name="
            t.kind === 'error'
              ? 'circle-alert'
              : t.kind === 'ok'
                ? 'circle-check'
                : 'info'
          "
          :size="16"
          class="mt-0.5"
          :class="
            t.kind === 'error'
              ? 'text-[#ff8a99]'
              : t.kind === 'ok'
                ? 'text-ok'
                : 'text-fog-300'
          "
        />
        <p class="min-w-0 flex-1 break-words text-fog-100">{{ t.text }}</p>
        <button
          type="button"
          class="-m-1 rounded p-1 text-fog-400 hover:text-fog-100"
          aria-label="Dismiss"
          @click="toasts.dismiss(t.id)"
        >
          <AppIcon name="x" :size="14" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 180ms ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
</style>
