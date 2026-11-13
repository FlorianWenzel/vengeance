<script setup lang="ts">
import { onMounted } from "vue";
import AppShell from "./components/AppShell.vue";
import GrainOverlay from "./components/GrainOverlay.vue";
import ToastHost from "./components/ToastHost.vue";
import BrandMark from "./components/BrandMark.vue";
import SearchBeam from "./components/SearchBeam.vue";
import AppIcon from "./components/AppIcon.vue";
import { useSession } from "./stores/session";

const session = useSession();
onMounted(() => session.load());
</script>

<template>
  <GrainOverlay />
  <AppShell v-if="session.me" />
  <div
    v-else
    class="relative grid min-h-dvh place-items-center overflow-hidden px-4"
  >
    <SearchBeam :angle="-14" />
    <div class="relative text-center">
      <div class="mx-auto mb-4 w-fit text-fog-400">
        <BrandMark :size="48" />
      </div>
      <p
        class="font-display text-3xl font-bold tracking-[0.3em] text-fog-100 uppercase"
      >
        Vengeance
      </p>
      <div v-if="session.error" class="mx-auto mt-6 max-w-sm" role="alert">
        <p class="text-sm text-fog-300">{{ session.error }}</p>
        <button type="button" class="btn mt-4" @click="session.load()">
          <AppIcon name="refresh-cw" :size="14" />Try again
        </button>
      </div>
      <p v-else class="mt-3 text-sm text-fog-400" role="status">
        Sweeping the streets…
      </p>
    </div>
  </div>
  <ToastHost />
</template>
