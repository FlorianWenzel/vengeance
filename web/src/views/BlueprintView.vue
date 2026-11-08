<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { RouterLink } from 'vue-router'
import AppIcon from '../components/AppIcon.vue'
import LoadingBlock from '../components/LoadingBlock.vue'
import ErrorBlock from '../components/ErrorBlock.vue'
import EmptyState from '../components/EmptyState.vue'
import { api, errorMessage } from '../lib/api'
import { ago, plural } from '../lib/format'
import type { Scene, StatusLevel, TopoStatus } from '../lib/types'

// The renderer is Rust → WASM (viz/), built into src/viz/pkg by scripts/build-viz.sh.
// import.meta.glob keeps the web build working when it hasn't been built (dev without Rust).
const vizModules = import.meta.glob('../viz/pkg/blueprint.js')

interface BlueprintApi {
  set_scene(json: string): void
  resize(w: number, h: number, dpr: number): void
  frame(t: number): void
  pointer_down(x: number, y: number, b: number): void
  pointer_move(x: number, y: number): void
  pointer_up(x: number, y: number, b: number): void
  wheel(dy: number, x: number, y: number): void
  key(code: string): void
  focus(id: string): void
  reset_view(): void
  hovered(): string | undefined
  selected(): string | undefined
  labels(): string
  set_reduced_motion(on: boolean): void
  free(): void
}
interface Label {
  id: string
  text: string
  sub?: string
  x: number
  y: number
  depth: number
  level?: string
  kind: string
  anchor?: 'above' | 'right' | 'inside' | 'below'
}

const scene = shallowRef<Scene | null>(null)
const loading = ref(true)
const loadError = ref<string | null>(null)
const vizError = ref<string | null>(null)
const view = ref<'3d' | 'list'>(vizModules['../viz/pkg/blueprint.js'] ? '3d' : 'list')
const canvas = ref<HTMLCanvasElement | null>(null)
const stage = ref<HTMLDivElement | null>(null)
const labels = ref<Label[]>([])
const hoveredId = ref<string | undefined>()
const selectedId = ref<string | undefined>()
let bp: BlueprintApi | null = null
let raf = 0
let frameNo = 0
let poll = 0
let ro: ResizeObserver | null = null

async function load() {
  try {
    const s = await api.get<Scene>('/api/v1/topology')
    scene.value = s
    loadError.value = null
    bp?.set_scene(JSON.stringify(s))
  } catch (e) {
    loadError.value = errorMessage(e)
  } finally {
    loading.value = false
  }
}

const hasTopology = computed(() => (scene.value?.hosts ?? []).some((h) => h.groups.length > 0))

async function startViz() {
  if (bp || !canvas.value || !scene.value) return
  const loader = vizModules['../viz/pkg/blueprint.js']
  if (!loader) {
    vizError.value = 'The 3D renderer is not part of this build.'
    view.value = 'list'
    return
  }
  try {
    const mod = (await loader()) as { default: () => Promise<unknown>; Blueprint: new (c: HTMLCanvasElement) => BlueprintApi }
    await mod.default()
    bp = new mod.Blueprint(canvas.value)
    bp.set_reduced_motion(window.matchMedia('(prefers-reduced-motion: reduce)').matches)
    sizeCanvas()
    bp.set_scene(JSON.stringify(scene.value))
    ro = new ResizeObserver(sizeCanvas)
    if (stage.value) ro.observe(stage.value)
    const loop = (t: number) => {
      if (!bp) return
      bp.frame(t)
      if (++frameNo % 2 === 0) {
        try {
          labels.value = JSON.parse(bp.labels()) as Label[]
        } catch {
          labels.value = []
        }
        hoveredId.value = bp.hovered()
        const sel = bp.selected()
        if (sel !== selectedId.value) selectedId.value = sel
      }
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
  } catch (e) {
    vizError.value = `3D view unavailable: ${errorMessage(e)}. Showing the list instead.`
    view.value = 'list'
    bp = null
  }
}

function stopViz() {
  cancelAnimationFrame(raf)
  ro?.disconnect()
  ro = null
  bp?.free()
  bp = null
  labels.value = []
}

function sizeCanvas() {
  if (!bp || !stage.value) return
  const r = stage.value.getBoundingClientRect()
  bp.resize(r.width, r.height, window.devicePixelRatio || 1)
}

function local(e: PointerEvent | WheelEvent) {
  const r = canvas.value!.getBoundingClientRect()
  return [e.clientX - r.left, e.clientY - r.top] as const
}
function onDown(e: PointerEvent) {
  canvas.value?.setPointerCapture(e.pointerId)
  const [x, y] = local(e)
  bp?.pointer_down(x, y, e.button)
}
function onMove(e: PointerEvent) {
  const [x, y] = local(e)
  bp?.pointer_move(x, y)
}
function onUp(e: PointerEvent) {
  const [x, y] = local(e)
  bp?.pointer_up(x, y, e.button)
}
function onWheel(e: WheelEvent) {
  const [x, y] = local(e)
  bp?.wheel(e.deltaY, x, y)
}
function onKey(e: KeyboardEvent) {
  if (['Escape', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
    e.preventDefault()
    bp?.key(e.key)
  }
}

function focusId(id: string) {
  selectedId.value = id
  if (view.value === '3d') bp?.focus(id)
}
function resetView() {
  selectedId.value = undefined
  bp?.reset_view()
}

watch(view, async (v) => {
  if (v === '3d') {
    await nextTick()
    startViz()
  } else stopViz()
})

onMounted(async () => {
  await load()
  poll = window.setInterval(load, 60_000)
  if (view.value === '3d' && hasTopology.value) {
    await nextTick()
    startViz()
  }
})
onBeforeUnmount(() => {
  clearInterval(poll)
  stopViz()
})

// --- selection details -------------------------------------------------------
type Found =
  | { kind: 'host'; name: string; sub: string; status: TopoStatus; assetId: number | null; children: number }
  | { kind: 'group'; name: string; sub: string; status: TopoStatus; workloads: { id: string; name: string; ready: number; desired: number; status: TopoStatus }[] }
  | { kind: 'workload'; name: string; sub: string; status: TopoStatus; images: { id: string; name: string; ref: string; assetId: number | null; status: TopoStatus }[] }
  | { kind: 'image'; name: string; sub: string; status: TopoStatus; assetId: number | null }
  | { kind: 'external'; name: string; sub: string; status: TopoStatus; assetId: number }

const found = computed<Found | null>(() => {
  const id = selectedId.value
  const s = scene.value
  if (!id || !s) return null
  for (const h of s.hosts) {
    if (h.id === id) return { kind: 'host', name: h.name, sub: h.subtitle, status: h.status, assetId: h.assetId, children: h.groups.length }
    for (const g of h.groups) {
      if (g.id === id) return { kind: 'group', name: g.name, sub: `namespace on ${h.name}`, status: g.status, workloads: g.workloads }
      for (const w of g.workloads) {
        if (w.id === id) return { kind: 'workload', name: w.name, sub: `${w.kind} in ${g.name} · ${w.ready}/${w.desired} ready`, status: w.status, images: w.images }
        for (const i of w.images) if (i.id === id) return { kind: 'image', name: i.name, sub: i.ref, status: i.status, assetId: i.assetId }
      }
    }
  }
  for (const x of s.external) if (x.id === id) return { kind: 'external', name: x.name, sub: x.kind, status: x.status, assetId: x.assetId }
  return null
})

const LEVEL_LABEL: Record<StatusLevel, string> = { p1: 'P1 open', p2: 'P2 open', p3: 'P3 open', p4: 'P4 only', ok: 'Clean', unknown: 'Not scanned' }
const LEVEL_VAR: Record<StatusLevel, string> = {
  p1: 'var(--color-p1)', p2: 'var(--color-p2)', p3: 'var(--color-p3)', p4: 'var(--color-p4)', ok: 'var(--color-ok)', unknown: '#5b6478',
}
const legend: StatusLevel[] = ['p1', 'p2', 'p3', 'p4', 'ok', 'unknown']

const jumpTargets = computed(() => {
  const out: { id: string; label: string }[] = []
  for (const h of scene.value?.hosts ?? []) {
    out.push({ id: h.id, label: h.name })
    for (const g of h.groups) out.push({ id: g.id, label: `  ${g.name}` })
  }
  return out
})
const jump = ref('')
watch(jump, (v) => {
  if (v) focusId(v)
  jump.value = ''
})

const totals = computed(() => {
  let groups = 0
  let workloads = 0
  for (const h of scene.value?.hosts ?? []) {
    groups += h.groups.length
    for (const g of h.groups) workloads += g.workloads.length
  }
  return { hosts: scene.value?.hosts.length ?? 0, groups, workloads }
})

function findingsLink(assetId: number | null) {
  return assetId ? `/findings?asset=${assetId}` : null
}
</script>

<template>
  <div class="flex h-[calc(100dvh-4rem)] min-h-[32rem] flex-col gap-3 lg:h-[calc(100dvh-3rem)]">
    <header class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <p class="eyebrow">Inventory</p>
        <h1 class="font-display text-3xl tracking-wide text-fog-100 uppercase">Blueprint</h1>
        <p v-if="scene" class="text-sm text-fog-400">
          {{ plural(totals.hosts, 'server') }} · {{ plural(totals.groups, 'app') }} · {{ plural(totals.workloads, 'workload') }} ·
          updated {{ ago(scene.generatedAt) }}
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <label class="sr-only" for="bp-jump">Jump to</label>
        <select v-if="view === '3d'" id="bp-jump" v-model="jump" class="input w-48" data-testid="blueprint-jump">
          <option value="">Jump to…</option>
          <option v-for="t in jumpTargets" :key="t.id" :value="t.id">{{ t.label }}</option>
        </select>
        <button v-if="view === '3d'" type="button" class="btn btn-ghost btn-sm" data-testid="blueprint-reset" @click="resetView">
          <AppIcon name="rotate-ccw" :size="13" />Overview
        </button>
        <div class="inline-flex rounded-[4px] border border-ink-600" role="group" aria-label="View">
          <button type="button" class="btn btn-sm !rounded-none !border-0" :class="view === '3d' ? 'bg-ink-700 text-fog-100' : 'btn-ghost'" :aria-pressed="view === '3d'" data-testid="blueprint-view-3d" @click="view = '3d'">3D</button>
          <button type="button" class="btn btn-sm !rounded-none !border-0" :class="view === 'list' ? 'bg-ink-700 text-fog-100' : 'btn-ghost'" :aria-pressed="view === 'list'" data-testid="blueprint-view-list" @click="view = 'list'">List</button>
        </div>
      </div>
    </header>

    <LoadingBlock v-if="loading" :rows="4" label="Loading the blueprint" />
    <ErrorBlock v-else-if="loadError && !scene" :message="loadError" @retry="load" />
    <EmptyState v-else-if="!hasTopology" icon="server" title="No topology reported yet" data-testid="blueprint-empty">
      The blueprint draws what the nightly node scan reports: servers, the apps on them and their containers. Install
      <code>vengeance-scan</code> and <code>vengeance-topology</code> on each node (see <code>deploy/host/README.md</code>), or wait for the next nightly run.
    </EmptyState>

    <div v-else class="relative flex min-h-0 flex-1 gap-3">
      <p v-if="vizError" role="status" class="absolute top-2 left-2 z-10 rounded-[4px] border border-warn/40 bg-ink-900/90 px-3 py-2 text-sm text-warn">{{ vizError }}</p>

      <!-- 3D -->
      <div v-if="view === '3d'" ref="stage" class="bp-stage relative min-w-0 flex-1 overflow-hidden rounded-[6px] border border-ink-600" data-testid="blueprint-stage">
        <canvas
          ref="canvas"
          class="absolute inset-0 h-full w-full touch-none outline-none"
          :class="hoveredId ? 'cursor-pointer' : 'cursor-grab'"
          tabindex="0"
          role="img"
          aria-label="3D blueprint of servers and apps. Drag to orbit, scroll to zoom, click to select. The list view shows the same information as text."
          data-testid="blueprint-canvas"
          @pointerdown="onDown"
          @pointermove="onMove"
          @pointerup="onUp"
          @pointerleave="bp?.pointer_move(-1, -1)"
          @wheel.prevent="onWheel"
          @keydown="onKey"
          @contextmenu.prevent
        />
        <div class="pointer-events-none absolute inset-0" aria-hidden="true">
          <button
            v-for="l in labels"
            :key="l.id"
            type="button"
            tabindex="-1"
            class="bp-label pointer-events-auto absolute"
            :class="[`bp-${l.kind}`, `bp-at-${l.anchor ?? 'above'}`, { 'bp-active': l.id === selectedId || l.id === hoveredId }]"
            :style="{ transform: `translate(${Math.round(l.x)}px, ${Math.round(l.y)}px)`, opacity: Math.max(0.65, 1 - l.depth * 0.35), zIndex: Math.round((1 - l.depth) * 100) }"
            @click="focusId(l.id)"
          >
            <span class="bp-led" :style="{ background: LEVEL_VAR[(l.level as StatusLevel) || 'unknown'] }" />
            <span class="bp-text">{{ l.text }}</span>
            <span v-if="l.sub" class="bp-sub">{{ l.sub }}</span>
          </button>
        </div>
        <ul class="absolute bottom-2 left-2 flex flex-wrap gap-x-3 gap-y-1 rounded-[4px] bg-ink-950/80 px-2.5 py-1.5 text-[0.7rem] text-fog-300" aria-label="Legend">
          <li v-for="lv in legend" :key="lv" class="flex items-center gap-1.5"><span class="bp-led" :style="{ background: LEVEL_VAR[lv] }" />{{ LEVEL_LABEL[lv] }}</li>
          <li class="flex items-center gap-1.5 text-fog-500">blinking = scan missing or stale</li>
        </ul>
        <p class="absolute top-2 right-2 hidden rounded-[4px] bg-ink-950/80 px-2.5 py-1.5 text-[0.7rem] text-fog-500 xl:block">
          drag to orbit · shift-drag to pan · scroll to zoom · click to inspect · Esc to zoom out
        </p>
      </div>

      <!-- List (also the accessible / no-WebGL view) -->
      <div v-else class="min-w-0 flex-1 overflow-y-auto rounded-[6px] border border-ink-600 p-4" data-testid="blueprint-list">
        <section v-for="h in scene!.hosts" :key="h.id" class="mb-6">
          <button type="button" class="flex w-full items-center gap-2 text-left" data-testid="blueprint-tree-item" :data-id="h.id" @click="focusId(h.id)">
            <span class="bp-led" :style="{ background: LEVEL_VAR[h.status.level] }" />
            <span class="font-display text-lg tracking-wide text-fog-100 uppercase">{{ h.name }}</span>
            <span class="text-xs text-fog-500">{{ h.subtitle }}</span>
          </button>
          <ul class="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            <li v-for="g in h.groups" :key="g.id" class="rounded-[4px] border border-ink-600 p-2.5">
              <button type="button" class="flex w-full items-center gap-2 text-left" data-testid="blueprint-tree-item" :data-id="g.id" @click="focusId(g.id)">
                <span class="bp-led" :style="{ background: LEVEL_VAR[g.status.level] }" /><span class="font-medium text-fog-100">{{ g.name }}</span>
                <span v-if="g.status.open.total" class="ml-auto font-mono text-[0.7rem] text-fog-400">{{ g.status.open.total }} open</span>
              </button>
              <ul class="mt-1.5 space-y-1 text-sm">
                <li v-for="w in g.workloads" :key="w.id">
                  <button type="button" class="flex w-full items-center gap-2 text-left text-fog-300 hover:text-fog-100" data-testid="blueprint-tree-item" :data-id="w.id" @click="focusId(w.id)">
                    <span class="bp-led bp-led-sm" :style="{ background: LEVEL_VAR[w.status.level] }" />{{ w.name }}
                    <span class="font-mono text-[0.68rem]" :class="w.ready < w.desired ? 'text-warn' : 'text-fog-500'">{{ w.ready }}/{{ w.desired }}</span>
                  </button>
                </li>
              </ul>
            </li>
          </ul>
        </section>
        <section v-if="scene!.external.length">
          <p class="eyebrow mb-2">Outside the racks</p>
          <ul class="flex flex-wrap gap-2">
            <li v-for="x in scene!.external" :key="x.id">
              <button type="button" class="flex items-center gap-2 rounded-[4px] border border-ink-600 px-2 py-1 text-sm text-fog-300" data-testid="blueprint-tree-item" :data-id="x.id" @click="focusId(x.id)">
                <span class="bp-led bp-led-sm" :style="{ background: LEVEL_VAR[x.status.level] }" />{{ x.name }} <span class="text-fog-500">{{ x.kind }}</span>
              </button>
            </li>
          </ul>
        </section>
      </div>

      <!-- Details -->
      <aside v-if="found" class="w-80 shrink-0 overflow-y-auto rounded-[6px] border border-ink-600 bg-ink-900 p-4 max-lg:absolute max-lg:inset-y-0 max-lg:right-0 max-lg:z-20 max-lg:w-[min(20rem,100%)]" data-testid="blueprint-panel">
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <p class="eyebrow">{{ found.kind }}</p>
            <h2 class="font-display text-xl tracking-wide break-words text-fog-100 uppercase">{{ found.name }}</h2>
            <p class="text-xs break-all text-fog-400">{{ found.sub }}</p>
          </div>
          <button type="button" class="btn btn-ghost btn-sm" aria-label="Close details" @click="selectedId = undefined"><AppIcon name="x" :size="14" /></button>
        </div>
        <p class="mt-3 flex items-center gap-2 text-sm text-fog-200">
          <span class="bp-led" :style="{ background: LEVEL_VAR[found.status.level] }" />{{ LEVEL_LABEL[found.status.level] }}
          <span v-if="found.status.stale" class="text-warn">· scan missing or stale</span>
        </p>
        <dl class="mt-3 grid grid-cols-4 gap-1 text-center font-mono text-xs">
          <div v-for="p in (['p1', 'p2', 'p3', 'p4'] as const)" :key="p" class="rounded-[3px] bg-ink-800 py-1.5">
            <dt class="text-[0.62rem] text-fog-500 uppercase">{{ p }}</dt>
            <dd :style="{ color: found.status.open[p] ? LEVEL_VAR[p] : undefined }">{{ found.status.open[p] }}</dd>
          </div>
        </dl>
        <template v-if="found.kind === 'group'">
          <p class="eyebrow mt-4 mb-1.5">Workloads</p>
          <ul class="space-y-1 text-sm">
            <li v-for="w in found.workloads" :key="w.id">
              <button type="button" class="flex w-full items-center gap-2 text-left text-fog-300 hover:text-fog-100" @click="focusId(w.id)">
                <span class="bp-led bp-led-sm" :style="{ background: LEVEL_VAR[w.status.level] }" />{{ w.name }}
                <span class="ml-auto font-mono text-[0.68rem]" :class="w.ready < w.desired ? 'text-warn' : 'text-fog-500'">{{ w.ready }}/{{ w.desired }}</span>
              </button>
            </li>
          </ul>
        </template>
        <template v-if="found.kind === 'workload'">
          <p class="eyebrow mt-4 mb-1.5">Images</p>
          <ul class="space-y-2 text-sm">
            <li v-for="i in found.images" :key="i.id">
              <div class="flex items-center gap-2"><span class="bp-led bp-led-sm" :style="{ background: LEVEL_VAR[i.status.level] }" /><span class="text-fog-100">{{ i.name }}</span></div>
              <p class="font-mono text-[0.68rem] break-all text-fog-500">{{ i.ref }}</p>
              <RouterLink v-if="i.assetId" :to="`/findings?asset=${i.assetId}`" class="link text-xs">{{ i.status.open.total }} open findings</RouterLink>
              <span v-else class="text-xs text-fog-500">not in the inventory yet</span>
            </li>
          </ul>
        </template>
        <div v-if="'assetId' in found && findingsLink(found.assetId)" class="mt-4 flex flex-col gap-2">
          <RouterLink :to="findingsLink(found.assetId)!" class="btn btn-primary btn-sm justify-center" data-testid="blueprint-findings">Open findings</RouterLink>
          <RouterLink :to="`/assets/${found.assetId}`" class="btn btn-ghost btn-sm justify-center">Asset details</RouterLink>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.bp-stage {
  background: radial-gradient(ellipse at 50% 30%, #0f1d33 0%, #08101d 60%, #050a12 100%);
}
.bp-led {
  display: inline-block;
  width: 0.55rem;
  height: 0.55rem;
  border-radius: 1px;
  box-shadow: 0 0 6px currentColor;
  flex: none;
}
.bp-led-sm {
  width: 0.45rem;
  height: 0.45rem;
}
.bp-label {
  left: 0;
  top: 0;
  display: flex;
  align-items: baseline;
  gap: 0.35rem;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 0.68rem;
  line-height: 1.2;
  color: #cfe6ff;
  padding: 0.1rem 0.35rem;
  border: 1px solid rgb(120 190 255 / 0.25);
  background: rgb(5 12 24 / 0.72);
  border-radius: 2px;
  letter-spacing: 0.03em;
}
.bp-at-above {
  translate: -50% calc(-100% - 4px);
}
.bp-at-right {
  translate: 6px -50%;
}
.bp-at-inside {
  translate: 2px 2px;
  background: transparent;
  border-color: transparent;
  color: #e3f1ff;
  text-shadow: 0 0 3px #050c18, 0 0 6px #050c18;
}
.bp-at-below {
  translate: -50% 4px;
}
.bp-host {
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: #eaf4ff;
  border-color: rgb(120 190 255 / 0.5);
}
.bp-image,
.bp-workload {
  font-size: 0.62rem;
}
.bp-sub {
  color: #7f9bbd;
  font-size: 0.6rem;
}
.bp-active {
  border-color: var(--color-beam);
  color: var(--color-beam-hi);
}
.bp-led.bp-led {
  align-self: center;
}
</style>
