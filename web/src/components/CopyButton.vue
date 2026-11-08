<script setup lang="ts">
import { ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { copyText } from '../lib/clipboard'
import { useToasts } from '../stores/toasts'

const props = withDefaults(defineProps<{ text: string; label?: string; small?: boolean }>(), { label: 'Copy', small: true })
const copied = ref(false)
const toasts = useToasts()
let timer = 0

async function copy() {
  const ok = await copyText(props.text)
  if (!ok) {
    toasts.error('Could not copy. Select the text and copy it manually.')
    return
  }
  copied.value = true
  window.clearTimeout(timer)
  timer = window.setTimeout(() => (copied.value = false), 1600)
}
</script>

<template>
  <button type="button" class="btn" :class="small ? 'btn-sm' : ''" :aria-label="`${label} to clipboard`" data-testid="copy" @click="copy">
    <AppIcon :name="copied ? 'check' : 'copy'" :size="13" :class="copied ? 'text-ok' : ''" />
    <span>{{ copied ? 'Copied' : label }}</span>
    <span class="sr-only" aria-live="polite">{{ copied ? 'Copied to clipboard' : '' }}</span>
  </button>
</template>
