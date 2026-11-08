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
  set_mode?(mode: 'orbit' | 'walk'): void
  key_down?(code: string): void
  key_up?(code: string): void
  look?(dx: number, dy: number): void
  interact?(): void
  minimap?(): string
}
interface Minimap {
  player: { x: number; z: number; heading: number }
  bounds: { minX: number; minZ: number; maxX: number; maxZ: number }
  items: { id: string; kind: string; x: number; z: number; w: number; d: number; level?: string }[]
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
  anchor?: 'above' | 'right' | 'inside' | 'below' | 'center'
}

const scene = shallowRef<Scene | null>(null)
const loading = ref(true)
const loadError = ref<string | null>(null)
const vizError = ref<string | null>(null)
const view = ref<'3d' | 'walk' | 'list'>(vizModules['../viz/pkg/blueprint.js'] ? '3d' : 'list')
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
const walkSupported = ref(false)
const locked = ref(false)
const minimapEl = ref<HTMLCanvasElement | null>(null)
const finePointer = typeof window !== 'undefined' && window.matchMedia('(pointer: fine)').matches

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
    walkSupported.value = finePointer && typeof bp.set_mode === 'function' && 'requestPointerLock' in canvas.value
    bp.set_mode?.(view.value === 'walk' && walkSupported.value ? 'walk' : 'orbit')
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
      if (view.value === 'walk' && frameNo % 6 === 0) drawMinimap()
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
  if (view.value === 'walk') {
    if (locked.value) bp?.interact?.()
    else enterWalk()
    return
  }
  canvas.value?.setPointerCapture(e.pointerId)
  const [x, y] = local(e)
  bp?.pointer_down(x, y, e.button)
}
function onMove(e: PointerEvent) {
  if (view.value === 'walk') return
  const [x, y] = local(e)
  bp?.pointer_move(x, y)
}
function onUp(e: PointerEvent) {
  if (view.value === 'walk') return
  const [x, y] = local(e)
  bp?.pointer_up(x, y, e.button)
}
function onWheel(e: WheelEvent) {
  if (view.value === 'walk') return
  const [x, y] = local(e)
  bp?.wheel(e.deltaY, x, y)
}
function onKey(e: KeyboardEvent) {
  if (view.value === 'walk') return
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

watch(view, async (v, old) => {
  if (v === 'list') {
    exitWalk()
    stopViz()
    return
  }
  await nextTick()
  if (!bp) await startViz()
  if (v === 'walk') bp?.set_mode?.('walk')
  else if (old === 'walk') {
    exitWalk()
    bp?.set_mode?.('orbit')
  }
})

// --- walk mode ---------------------------------------------------------------
const WALK_KEYS = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ShiftLeft', 'ShiftRight', 'KeyE', 'Space'])
function enterWalk() {
  canvas.value?.requestPointerLock()
}
function exitWalk() {
  if (document.pointerLockElement) document.exitPointerLock()
}
function onLockChange() {
  locked.value = document.pointerLockElement === canvas.value
  if (!locked.value) for (const k of WALK_KEYS) bp?.key_up?.(k) // release held keys
}
function onLookMove(e: MouseEvent) {
  if (locked.value) bp?.look?.(e.movementX, e.movementY)
}
function onWalkKey(e: KeyboardEvent, down: boolean) {
  if (view.value !== 'walk' || !locked.value || !WALK_KEYS.has(e.code)) return
  e.preventDefault()
  if (down) {
    if (e.repeat) return
    if (e.code === 'KeyE') bp?.interact?.()
    else bp?.key_down?.(e.code)
  } else bp?.key_up?.(e.code)
}
const onKeyDownWin = (e: KeyboardEvent) => onWalkKey(e, true)
const onKeyUpWin = (e: KeyboardEvent) => onWalkKey(e, false)

const LEVEL_HEX: Record<StatusLevel, string> = { p1: '#ef4058', p2: '#f27a3a', p3: '#e6d34f', p4: '#7397c2', ok: '#5fb58d', unknown: '#5b6478' }
function drawMinimap() {
  const el = minimapEl.value
  if (!el || !bp?.minimap) return
  let m: Minimap
  try {
    m = JSON.parse(bp.minimap()) as Minimap
  } catch {
    return
  }
  const ctx = el.getContext('2d')
  if (!ctx) return
  const dpr = window.devicePixelRatio || 1
  const W = el.clientWidth
  const H = el.clientHeight
  if (el.width !== Math.round(W * dpr)) {
    el.width = Math.round(W * dpr)
    el.height = Math.round(H * dpr)
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, W, H)
  const bw = m.bounds.maxX - m.bounds.minX || 1
  const bd = m.bounds.maxZ - m.bounds.minZ || 1
  const sc = Math.min((W - 16) / bw, (H - 16) / bd)
  const ox = (W - bw * sc) / 2
  const oz = (H - bd * sc) / 2
  const X = (x: number) => ox + (x - m.bounds.minX) * sc
  const Z = (z: number) => oz + (z - m.bounds.minZ) * sc
  ctx.lineWidth = 1
  ctx.strokeStyle = 'rgba(120,190,255,0.45)'
  ctx.strokeRect(X(m.bounds.minX), Z(m.bounds.minZ), bw * sc, bd * sc)
  for (const it of m.items) {
    const x0 = X(it.x - it.w / 2)
    const z0 = Z(it.z - it.d / 2)
    ctx.fillStyle = 'rgba(80,150,220,0.18)'
    ctx.fillRect(x0, z0, it.w * sc, it.d * sc)
    ctx.strokeStyle = it.id === hoveredId.value || it.id === selectedId.value ? '#f2b04a' : 'rgba(150,210,255,0.8)'
    ctx.strokeRect(x0, z0, it.w * sc, it.d * sc)
    ctx.fillStyle = LEVEL_HEX[(it.level as StatusLevel) || 'unknown'] ?? LEVEL_HEX.unknown
    ctx.fillRect(X(it.x) - 2, Z(it.z) - 2, 4, 4)
  }
  ctx.save()
  ctx.translate(X(m.player.x), Z(m.player.z))
  ctx.rotate(m.player.heading)
  ctx.fillStyle = '#f2b04a'
  ctx.beginPath()
  ctx.moveTo(0, -7)
  ctx.lineTo(5, 5)
  ctx.lineTo(0, 2)
  ctx.lineTo(-5, 5)
  ctx.closePath()
  ctx.fill()
  ctx.restore()
}

const aimed = computed(() => (view.value === 'walk' && hoveredId.value ? nameOf(hoveredId.value) : null))
function nameOf(id: string): string | null {
  const s = scene.value
  if (!s) return null
  for (const h of s.hosts) {
    if (h.id === id) return h.name
    for (const g of h.groups) {
      if (g.id === id) return `${g.name} · ${h.name}`
      for (const w of g.workloads) {
        if (w.id === id) return `${w.name} · ${g.name}`
        for (const i of w.images) if (i.id === id) return `${i.name} · ${w.name}`
      }
    }
  }
  return s.external.find((x) => x.id === id)?.name ?? null
}


onMounted(async () => {
  document.addEventListener('pointerlockchange', onLockChange)
  document.addEventListener('mousemove', onLookMove)
  window.addEventListener('keydown', onKeyDownWin)
  window.addEventListener('keyup', onKeyUpWin)
  await load()
  poll = window.setInterval(load, 60_000)
  if (view.value !== 'list' && hasTopology.value) {
    await nextTick()
    startViz()
  }
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerlockchange', onLockChange)
  document.removeEventListener('mousemove', onLookMove)
  window.removeEventListener('keydown', onKeyDownWin)
  window.removeEventListener('keyup', onKeyUpWin)
  exitWalk()
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
          <button v-if="walkSupported" type="button" class="btn btn-sm !rounded-none !border-0" :class="view === 'walk' ? 'bg-ink-700 text-fog-100' : 'btn-ghost'" :aria-pressed="view === 'walk'" data-testid="blueprint-view-walk" @click="view = 'walk'">Walk</button>
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
      <div v-if="view !== 'list'" ref="stage" class="bp-stage relative min-w-0 flex-1 overflow-hidden rounded-[6px] border border-ink-600" data-testid="blueprint-stage">
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
        <template v-if="view === 'walk'">
          <div v-if="locked" class="pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2" aria-hidden="true">
            <div class="bp-crosshair" :class="{ 'bp-crosshair-on': aimed }" />
          </div>
          <p v-if="locked && aimed" class="pointer-events-none absolute top-[calc(50%+1.6rem)] left-1/2 -translate-x-1/2 rounded-[3px] bg-ink-950/80 px-2 py-1 font-mono text-xs text-beam-hi" data-testid="blueprint-aim">
            {{ aimed }} <span class="text-fog-400">· E to inspect</span>
          </p>
          <canvas ref="minimapEl" class="pointer-events-none absolute top-2 right-2 z-[250] h-40 w-40 rounded-[4px] border border-ink-600 bg-ink-950/75" aria-label="Floor plan" data-testid="blueprint-minimap" />
          <button
            v-if="!locked"
            type="button"
            class="absolute inset-0 z-[300] m-auto flex h-fit w-fit flex-col items-center gap-2 rounded-[6px] border border-beam/50 bg-ink-950/95 px-6 py-5 text-center"
            data-testid="blueprint-enter-walk"
            @click="enterWalk"
          >
            <span class="font-display text-xl tracking-wider text-fog-100 uppercase">Enter the datacenter</span>
            <span class="font-mono text-xs text-fog-400">WASD to walk · mouse to look · Shift to run · E or click to inspect · Esc to leave</span>
          </button>
        </template>
        <p v-if="view === '3d'" class="absolute top-2 right-2 hidden rounded-[4px] bg-ink-950/80 px-2.5 py-1.5 text-[0.7rem] text-fog-500 xl:block">
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
.bp-at-center {
  translate: -50% -50%;
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
<style scoped>
.bp-crosshair {
  width: 18px;
  height: 18px;
  position: relative;
}
.bp-crosshair::before,
.bp-crosshair::after {
  content: '';
  position: absolute;
  background: rgb(207 230 255 / 0.85);
  box-shadow: 0 0 4px #050c18;
}
.bp-crosshair::before {
  left: 8px;
  top: 0;
  width: 2px;
  height: 18px;
}
.bp-crosshair::after {
  top: 8px;
  left: 0;
  width: 18px;
  height: 2px;
}
.bp-crosshair-on::before,
.bp-crosshair-on::after {
  background: var(--color-beam);
}
</style>
