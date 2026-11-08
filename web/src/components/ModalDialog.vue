<script setup lang="ts">
import { ref, toRef, useId } from 'vue'
import { useFocusTrap } from '../lib/useFocusTrap'

const props = withDefaults(defineProps<{ open: boolean; title: string; width?: string; testid?: string }>(), {
  width: '28rem',
})
const emit = defineEmits<{ close: [] }>()
const panel = ref<HTMLElement | null>(null)
const titleId = useId()
useFocusTrap(toRef(props, 'open'), panel, () => emit('close'))
</script>

<template>
  <Teleport to="body">
    <Transition name="dlg">
      <div v-if="open" class="fixed inset-0 z-50 flex items-end justify-center p-3 sm:items-center sm:p-6">
        <div class="absolute inset-0 bg-ink-950/75 backdrop-blur-[2px]" @click="emit('close')" />
        <div
          ref="panel"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          tabindex="-1"
          :data-testid="testid"
          class="dlg-panel relative flex max-h-[90vh] w-full flex-col overflow-hidden rounded-md border border-ink-600 bg-ink-850 shadow-[0_30px_80px_-20px_rgba(0,0,0,.8)] outline-none"
          :style="{ maxWidth: width }"
        >
          <div class="flex items-center justify-between gap-4 border-b border-ink-700 px-5 py-3.5">
            <h2 :id="titleId" class="text-lg text-fog-100">{{ title }}</h2>
            <button type="button" class="btn btn-ghost btn-icon -mr-2" aria-label="Close" @click="emit('close')">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" /></svg>
            </button>
          </div>
          <div class="overflow-y-auto px-5 py-4">
            <slot />
          </div>
          <div v-if="$slots.footer" class="flex flex-wrap items-center justify-end gap-2 border-t border-ink-700 bg-ink-900/60 px-5 py-3">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dlg-enter-active,
.dlg-leave-active {
  transition: opacity 140ms ease;
}
.dlg-enter-active .dlg-panel,
.dlg-leave-active .dlg-panel {
  transition: transform 160ms ease;
}
.dlg-enter-from,
.dlg-leave-to {
  opacity: 0;
}
.dlg-enter-from .dlg-panel {
  transform: translateY(8px);
}
</style>
