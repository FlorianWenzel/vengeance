<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import AppIcon from './AppIcon.vue'
import BrandMark from './BrandMark.vue'
import { useSession } from '../stores/session'
import type { IconName } from '../lib/icons'

const session = useSession()
const route = useRoute()
const menuOpen = ref(false)
watch(
  () => route.fullPath,
  () => (menuOpen.value = false),
)

interface NavItem {
  to: string
  label: string
  icon: IconName
  testid?: string
  match: (p: string) => boolean
}

const primary: NavItem[] = [
  { to: '/', label: 'Overview', icon: 'layout-dashboard', testid: 'nav-overview', match: (p) => p === '/' },
  { to: '/findings', label: 'Findings', icon: 'inbox', testid: 'nav-findings', match: (p) => p.startsWith('/findings') },
  { to: '/fixes', label: 'Fixes', icon: 'wrench', testid: 'nav-fixes', match: (p) => p.startsWith('/fixes') },
]
const inventory: NavItem[] = [
  { to: '/assets', label: 'Assets', icon: 'server', testid: 'nav-assets', match: (p) => p.startsWith('/assets') },
  { to: '/projects', label: 'Projects', icon: 'folder', testid: 'nav-projects', match: (p) => p.startsWith('/projects') },
  { to: '/coverage', label: 'Coverage', icon: 'radar', testid: 'nav-coverage', match: (p) => p.startsWith('/coverage') },
]
const setup: NavItem[] = [
  { to: '/sources', label: 'Sources', icon: 'radio-tower', testid: 'nav-sources', match: (p) => p.startsWith('/sources') },
  { to: '/notifications', label: 'Notifications', icon: 'bell', testid: 'nav-notifications', match: (p) => p.startsWith('/notifications') },
]
const groups = [
  { label: 'Triage', items: primary },
  { label: 'Inventory', items: inventory },
  { label: 'Setup', items: setup },
]
</script>

<template>
  <div class="min-h-dvh lg:pl-60">
    <a href="#main" class="sr-only z-50 rounded bg-beam px-3 py-2 text-beam-ink focus:not-sr-only focus:fixed focus:top-2 focus:left-2">Skip to content</a>

    <!-- Mobile top bar -->
    <header class="sticky top-0 z-30 flex h-12 items-center gap-3 border-b border-ink-700 bg-ink-900/95 px-3 backdrop-blur lg:hidden">
      <button type="button" class="btn btn-ghost btn-icon" :aria-expanded="menuOpen" aria-controls="sidebar" aria-label="Menu" @click="menuOpen = !menuOpen">
        <AppIcon :name="menuOpen ? 'x' : 'menu'" :size="18" />
      </button>
      <RouterLink to="/" class="flex items-center gap-2 text-fog-300">
        <BrandMark :size="22" />
        <span class="font-display text-lg font-bold tracking-[0.22em] text-fog-100 uppercase">Vengeance</span>
      </RouterLink>
    </header>

    <div v-if="menuOpen" class="fixed inset-0 z-30 bg-ink-950/70 lg:hidden" @click="menuOpen = false" />

    <aside
      id="sidebar"
      class="fixed inset-y-0 left-0 z-40 flex w-60 flex-col border-r border-ink-700 bg-ink-950 transition-transform lg:translate-x-0"
      :class="menuOpen ? 'translate-x-0' : '-translate-x-full'"
    >
      <div class="relative overflow-hidden px-5 pt-5 pb-6">
        <!-- the lamp: a small cone of light over the wordmark -->
        <div class="lamp" aria-hidden="true" />
        <RouterLink to="/" class="relative flex items-center gap-2.5 text-fog-400">
          <BrandMark :size="30" />
          <span class="font-display text-[1.45rem] leading-none font-extrabold tracking-[0.24em] text-fog-100 uppercase">Vengeance</span>
        </RouterLink>
        <p class="relative mt-1.5 pl-[2.6rem] text-[0.68rem] tracking-[0.12em] text-fog-500 uppercase">Security inbox</p>
      </div>

      <nav class="flex-1 overflow-y-auto px-3 pb-4" aria-label="Main">
        <div v-for="g in groups" :key="g.label" class="mb-4">
          <p class="eyebrow mb-1 px-2.5 !text-[0.68rem] !text-fog-500">{{ g.label }}</p>
          <ul>
            <li v-for="item in g.items" :key="item.to">
              <RouterLink
                :to="item.to"
                :data-testid="item.testid"
                class="navlink"
                :class="item.match(route.path) ? 'is-active' : ''"
                :aria-current="item.match(route.path) ? 'page' : undefined"
              >
                <AppIcon :name="item.icon" :size="16" />
                <span>{{ item.label }}</span>
              </RouterLink>
            </li>
          </ul>
        </div>
      </nav>

      <div class="border-t border-ink-700 px-4 py-3">
        <div class="flex items-center gap-2.5">
          <div class="grid h-8 w-8 shrink-0 place-items-center rounded-full border border-ink-600 bg-ink-850 text-fog-300">
            <AppIcon name="user" :size="15" />
          </div>
          <div class="min-w-0 flex-1">
            <p v-if="session.me?.name" class="truncate text-xs font-medium text-fog-100">{{ session.me.name }}</p>
            <p class="truncate text-xs text-fog-400" data-testid="user-email" :title="session.me?.email">{{ session.me?.email }}</p>
          </div>
          <button type="button" class="btn btn-ghost btn-icon" data-testid="sign-out" aria-label="Sign out" title="Sign out" @click="session.signOut()">
            <AppIcon name="log-out" :size="15" />
          </button>
        </div>
        <p class="mt-3 text-[0.65rem] text-fog-500">
          Open source under
          <a class="underline decoration-ink-600 underline-offset-2 hover:text-fog-300" href="https://www.gnu.org/licenses/agpl-3.0.html" rel="noopener" target="_blank">AGPL-3.0</a>
        </p>
      </div>
    </aside>

    <main id="main" class="mx-auto w-full max-w-[96rem] px-4 pt-5 pb-16 sm:px-6 lg:px-8 lg:pt-7">
      <RouterView v-slot="{ Component }">
        <component :is="Component" />
      </RouterView>
    </main>
  </div>
</template>

<style scoped>
.navlink {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  height: 2.1rem;
  padding: 0 0.65rem;
  border-radius: 4px;
  font-size: 0.85rem;
  color: var(--color-fog-300);
  position: relative;
  transition:
    color 120ms,
    background-color 120ms;
}
.navlink:hover {
  color: var(--color-fog-100);
  background: var(--color-ink-850);
}
.navlink.is-active {
  color: var(--color-fog-100);
  background: linear-gradient(90deg, color-mix(in oklab, var(--color-beam) 13%, transparent), transparent 85%);
}
.navlink.is-active::before {
  content: '';
  position: absolute;
  left: -0.75rem;
  top: 0.35rem;
  bottom: 0.35rem;
  width: 2px;
  border-radius: 1px;
  background: var(--color-beam);
  box-shadow: 0 0 10px var(--color-beam);
}
.navlink.is-active :deep(svg) {
  color: var(--color-beam);
}
.lamp {
  position: absolute;
  top: -40px;
  left: -30px;
  width: 260px;
  height: 160px;
  background: radial-gradient(ellipse 60% 70% at 30% 20%, color-mix(in oklab, var(--color-beam) 18%, transparent), transparent 70%);
  pointer-events: none;
}
</style>
