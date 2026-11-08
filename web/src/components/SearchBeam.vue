<script setup lang="ts">
// Decorative diagonal light cone. Purely visual; hidden from assistive tech.
withDefaults(defineProps<{ angle?: number; intensity?: number }>(), { angle: -24, intensity: 1 })
</script>

<template>
  <div class="beam-root" aria-hidden="true">
    <div class="beam-cone" :style="{ transform: `rotate(${angle}deg)`, opacity: 0.16 * intensity }" />
    <div class="beam-source" :style="{ opacity: 0.5 * intensity }" />
  </div>
</template>

<style scoped>
.beam-root {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}
.beam-cone {
  position: absolute;
  left: 50%;
  bottom: -10%;
  width: 70%;
  height: 160%;
  margin-left: -35%;
  transform-origin: 50% 100%;
  clip-path: polygon(46% 100%, 54% 100%, 100% 0, 0 0);
  background: linear-gradient(to top, var(--color-beam) 0%, color-mix(in oklab, var(--color-beam) 35%, transparent) 45%, transparent 85%);
  filter: blur(18px);
  animation: sweep 14s ease-in-out infinite alternate;
}
.beam-source {
  position: absolute;
  left: 50%;
  bottom: -40px;
  width: 160px;
  height: 80px;
  margin-left: -80px;
  border-radius: 50%;
  background: radial-gradient(closest-side, var(--color-beam), transparent);
  filter: blur(10px);
}
@keyframes sweep {
  from {
    translate: -4% 0;
  }
  to {
    translate: 4% 0;
  }
}
</style>
