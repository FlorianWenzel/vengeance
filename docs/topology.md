# Infrastructure blueprint: topology data and renderer contract

The **Blueprint** page shows the infrastructure as physical hardware in 3D: every host is a rack
cabinet, every namespace (app) a rack unit inside it, every workload a module in that unit, every
container image a small chip on the module. Status lights show the worst open finding. Zooming in
reveals more detail. The renderer is Rust compiled to WebAssembly drawing with WebGL2
(`viz/` crate), the page around it is Vue.

## 1. Topology ingest (nightly scan → server)

`POST /api/v1/topology` with a **source token** (`Authorization: Bearer vg_…`), body:

```json
{
  "cluster": "slowbloom",
  "nodes": [
    { "name": "slowbloom", "host": "host:slowbloom-node", "cpu": "4", "memory": "7936204Ki",
      "os": "Ubuntu 24.04.5 LTS", "kernel": "6.8.0-149-generic", "kubelet": "v1.37.2+k3s1" }
  ],
  "workloads": [
    { "namespace": "slowbloom", "name": "api", "kind": "Deployment", "node": "slowbloom",
      "ready": 1, "desired": 1,
      "containers": [ { "name": "api", "image": "registry.flos.life/petal-api:f489ff4", "imageId": "sha256:…" } ] }
  ]
}
```

`host` is the Vengeance asset reference for the node (`kind:identifier`). The server stores the
latest snapshot per cluster (replacing the previous one) and answers `{ "ok": true }`.

## 2. Scene (server → page → renderer)

`GET /api/v1/topology` (session auth) returns the **scene**. Every element has a stable string `id`
and a `status`. Image references are already resolved to Vengeance assets.

```ts
type Status = {
  level: 'p1' | 'p2' | 'p3' | 'p4' | 'ok' | 'unknown'  // worst open priority; ok = nothing open; unknown = never scanned
  open: { p1: number; p2: number; p3: number; p4: number; total: number }
  stale: boolean        // an expected scan is missing or too old
}

type Scene = {
  generatedAt: string
  hosts: Host[]
  external: External[]  // assets not running on a host: repositories, domains, registries
}

type Host = {
  id: string            // "host:<assetId>" or "node:<name>" when no asset matches
  assetId: number | null
  name: string          // display name
  subtitle: string      // e.g. "Ubuntu 24.04.5 LTS · 4 CPU · 7.6 GiB · k3s v1.37.2"
  status: Status        // the host's own findings (OS packages, binaries)
  cluster: string | null
  groups: Group[]       // namespaces, sorted by name
}

type Group = {
  id: string            // "ns:<cluster>/<namespace>"
  name: string          // namespace
  status: Status        // worst of its workloads' images
  workloads: Workload[]
}

type Workload = {
  id: string            // "wl:<cluster>/<namespace>/<kind>/<name>"
  name: string
  kind: string          // Deployment, StatefulSet, DaemonSet, Job, Pod
  ready: number
  desired: number
  status: Status        // worst of its images
  images: Image[]
}

type Image = {
  id: string            // "img:<assetId>" or "img:<image ref>" if unknown to Vengeance
  assetId: number | null
  name: string          // short name, e.g. "petal-api"
  ref: string           // full image reference as running
  status: Status
}

type External = {
  id: string            // "asset:<assetId>"
  assetId: number
  kind: 'repository' | 'domain' | 'registry' | 'image' | 'host' | 'cluster'
  name: string
  status: Status
}
```

## 3. Renderer API (Rust → wasm-bindgen → JS)

Package `vengeance_blueprint`, built with `wasm-bindgen --target web` into `web/src/viz/pkg/`.

```ts
class Blueprint {
  constructor(canvas: HTMLCanvasElement)          // creates a WebGL2 context; throws if unavailable
  set_scene(sceneJson: string): void              // (re)build geometry; keeps camera if ids persist
  resize(cssWidth: number, cssHeight: number, devicePixelRatio: number): void
  frame(timeMs: number): void                     // draw one frame (JS drives requestAnimationFrame)
  pointer_down(x: number, y: number, button: number): void   // CSS pixels relative to canvas
  pointer_move(x: number, y: number): void
  pointer_up(x: number, y: number, button: number): void
  wheel(deltaY: number, x: number, y: number): void
  key(code: string): void                         // "Escape" zooms out one level, arrows orbit
  focus(id: string): void                         // fly the camera to an element
  reset_view(): void
  hovered(): string | undefined                   // id under the pointer
  selected(): string | undefined                  // id last clicked (a click is down+up without a drag)
  labels(): string                                // JSON: [{ id, text, sub, x, y, depth, level, kind }] for HTML overlay
  free(): void
}
```

- Orbit with left-drag, pan with right-drag or shift-drag, zoom with the wheel towards the pointer.
  Clicking an element selects it and flies to it; clicking empty space clears the selection.
- `labels()` returns only labels that should be visible at the current zoom: hosts always; groups when
  the camera is close enough to read them; workloads and images when zoomed into a group.
  `x`/`y` are CSS pixels; `depth` 0..1 for sorting and fading.
- Blueprint look: dark navy background with a fine grid on the floor plane, cyan/white wireframe
  outlines with faint translucent faces, an amber outline for hovered/selected, status LEDs coloured
  p1 `#ff4d5e`, p2 `#ff8a3d`, p3 `#ffd166`, p4 `#7da7d9`, ok `#3ddc97`, unknown `#5b6478`.
  Stale elements blink slowly. Respect a `reduced_motion` flag (set via `set_reduced_motion(bool)`).
- No text rendering in WebGL: all text is the HTML overlay built from `labels()`.
