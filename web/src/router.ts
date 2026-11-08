import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  { path: '/', name: 'overview', component: () => import('./views/OverviewView.vue'), meta: { title: 'Overview' } },
  { path: '/findings', name: 'findings', component: () => import('./views/FindingsView.vue'), meta: { title: 'Findings' } },
  { path: '/fixes', name: 'fixes', component: () => import('./views/FixesView.vue'), meta: { title: 'Fixes' } },
  { path: '/assets', name: 'assets', component: () => import('./views/AssetsView.vue'), meta: { title: 'Assets' } },
  {
    path: '/assets/:id(\\d+)',
    name: 'asset',
    component: () => import('./views/AssetDetailView.vue'),
    props: (r) => ({ id: Number(r.params.id) }),
    meta: { title: 'Asset' },
  },
  { path: '/blueprint', name: 'blueprint', component: () => import('./views/BlueprintView.vue'), meta: { title: 'Blueprint' } },
  { path: '/projects', name: 'projects', component: () => import('./views/ProjectsView.vue'), meta: { title: 'Projects' } },
  { path: '/coverage', name: 'coverage', component: () => import('./views/CoverageView.vue'), meta: { title: 'Coverage' } },
  { path: '/sources', name: 'sources', component: () => import('./views/SourcesView.vue'), meta: { title: 'Sources' } },
  {
    path: '/notifications',
    name: 'notifications',
    component: () => import('./views/NotificationsView.vue'),
    meta: { title: 'Notifications' },
  },
  { path: '/ai', name: 'ai', component: () => import('./views/AiView.vue'), meta: { title: 'AI assistants' } },
  { path: '/:pathMatch(.*)*', name: 'not-found', component: () => import('./views/NotFoundView.vue'), meta: { title: 'Not found' } },
]

export const router = createRouter({
  history: createWebHistory('/'),
  routes,
  scrollBehavior(to, from, saved) {
    if (saved) return saved
    // Query-only changes (filters, pagination, drawer) keep the scroll position.
    if (to.path === from.path) return false
    return { top: 0 }
  },
})

router.afterEach((to) => {
  const t = to.meta.title as string | undefined
  document.title = t ? `${t} · Vengeance` : 'Vengeance'
})
