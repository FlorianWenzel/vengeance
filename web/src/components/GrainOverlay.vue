<template>
  <!-- Faint film grain over the whole app. Inline SVG (no data: URIs) to stay CSP-friendly. -->
  <svg class="grain" aria-hidden="true" focusable="false">
    <filter id="vg-grain">
      <feTurbulence
        type="fractalNoise"
        baseFrequency="0.85"
        numOctaves="2"
        stitchTiles="stitch"
      />
      <feColorMatrix type="saturate" values="0" />
    </filter>
    <rect width="100%" height="100%" filter="url(#vg-grain)" />
  </svg>
</template>

<style scoped>
.grain {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  opacity: 0.045;
  z-index: 60;
  mix-blend-mode: screen;
}
</style>
