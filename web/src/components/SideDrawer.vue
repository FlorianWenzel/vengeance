<script setup lang="ts">
import { ref, toRef, useId } from 'vue'
import { useFocusTrap } from '../lib/useFocusTrap'

const props = withDefaults(defineProps<{ open: boolean; label: string; testid?: string }>(), {})
const emit = defineEmits<{ close: [] }>()
const panel = ref<HTMLElement | null>(null)
const labelId = useId()
useFocusTrap(toRef(props, 'open'), panel, () => emit('close'))
</script>

<template>
  <Teleport to="body">
    <Transition name="drw">
      <div v-if="open" class="fixed inset-0 z-40">
        <div class="absolute inset-0 bg-ink-950/60" @click="emit('close')" />
        <aside
          ref="panel"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="labelId"
          tabindex="-1"
          :data-testid="testid"
          class="drw-panel absolute inset-y-0 right-0 flex w-full max-w-[44rem] flex-col border-l border-ink-600 bg-ink-850 shadow-[-30px_0_80px_-30px_rgba(0,0,0,.9)] outline-none"
        >
          <span :id="labelId" class="sr-only">{{ label }}</span>
          <slot :close="() => emit('close')" />
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.drw-enter-active,
.drw-leave-active {
  transition: opacity 160ms ease;
}
.drw-enter-active .drw-panel,
.drw-leave-active .drw-panel {
  transition: transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
.drw-enter-from,
.drw-leave-to {
  opacity: 0;
}
.drw-enter-from .drw-panel,
.drw-leave-to .drw-panel {
  transform: translateX(24px);
}
</style>
