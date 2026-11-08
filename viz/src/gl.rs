//! WebGL2 drawing: a ray-cast floor grid, instanced box faces, instanced screen-space lines
//! (crisp at any DPR, with hidden-line passes) and additive LED glows.

use glam::Mat4;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlBuffer, WebGlProgram, WebGlShader, WebGlUniformLocation, WebGlVertexArrayObject};

use crate::engine::Engine;
use crate::geo::{FACE_STRIDE, LED_STRIDE, LINE_STRIDE};

const COMMON: &str = r#"
uniform mat4 uVP;
uniform vec3 uEye;
uniform vec2 uViewport;
uniform float uDpr;
uniform float uTime;
uniform float uMotion;
uniform vec2 uFog;
bool flag(float f, float bit) { return mod(floor(f / bit + 0.001), 2.0) >= 1.0; }
float lodVis(vec3 anchor, float nearD, float farD) {
  float d = distance(uEye, anchor);
  float o = farD <= 0.0 ? 1.0 : 1.0 - smoothstep(farD * 0.8, farD, d);
  float i = nearD <= 0.0 ? 1.0 : smoothstep(nearD * 0.8, nearD, d);
  return o * i;
}
float blinkMul(float flags) {
  if (!flag(flags, 1.0)) return 1.0;
  if (uMotion < 0.5) return 0.5;
  return 0.22 + 0.78 * (0.5 + 0.5 * cos(uTime * 6.2831853 / 2600.0));
}
float fogMul(vec3 p) {
  float d = distance(uEye, p);
  return 1.0 - 0.82 * clamp((d - uFog.x) / max(uFog.y - uFog.x, 0.001), 0.0, 1.0);
}
"#;

const LINE_VS: &str = r#"
layout(location=0) in vec2 aCorner;
layout(location=1) in vec3 aP0;
layout(location=2) in vec3 aP1;
layout(location=3) in vec4 aColor;
layout(location=4) in vec3 aAnchor;
layout(location=5) in vec4 aParams; // near, far, flags, width (css px)
uniform float uHidden;
uniform float uAlpha;
out vec4 vColor;
out float vSide;
out float vHalf;
out vec2 vAlong;
flat out float vDash;
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  float flags = aParams.z;
  vec4 c0 = uVP * vec4(aP0, 1.0);
  vec4 c1 = uVP * vec4(aP1, 1.0);
  float eps = 1e-3;
  if (vis < 0.004 || (uHidden > 0.5 && flag(flags, 16.0)) || (c0.w < eps && c1.w < eps)) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  if (c0.w < eps) c0 = mix(c0, c1, (eps - c0.w) / (c1.w - c0.w));
  else if (c1.w < eps) c1 = mix(c1, c0, (eps - c1.w) / (c0.w - c1.w));
  vec2 s0 = c0.xy / c0.w * 0.5 * uViewport;
  vec2 s1 = c1.xy / c1.w * 0.5 * uViewport;
  vec2 d = s1 - s0;
  float len = length(d);
  vec2 dir = len > 1e-4 ? d / len : vec2(1.0, 0.0);
  vec2 n = vec2(-dir.y, dir.x);
  float wpx = aParams.w * uDpr;
  float w = max(wpx, 1.0);
  float h = w * 0.5 + 1.0;
  vec4 c = aCorner.x < 0.5 ? c0 : c1;
  float ext = (aCorner.x * 2.0 - 1.0) * w * 0.5;
  vec2 off = n * aCorner.y * h + dir * ext;
  c.xy += off / (0.5 * uViewport) * c.w;
  gl_Position = c;
  float along = aCorner.x * len + ext;
  vAlong = vec2(along * c.w, c.w);
  vSide = aCorner.y * h;
  vHalf = w * 0.5;
  float a = aColor.a * vis * blinkMul(flags) * fogMul(mix(aP0, aP1, aCorner.x)) * uAlpha;
  if (wpx < 1.0) a *= wpx;
  vColor = vec4(aColor.rgb, a);
  vDash = (flag(flags, 2.0) || uHidden > 0.5) ? 1.0 : 0.0;
}
"#;

const LINE_FS: &str = r#"
uniform float uHidden;
in vec4 vColor;
in float vSide;
in float vHalf;
in vec2 vAlong;
flat in float vDash;
out vec4 outColor;
void main() {
  float cov = clamp(vHalf + 0.5 - abs(vSide), 0.0, 1.0);
  if (vDash > 0.5) {
    float s = vAlong.x / vAlong.y;
    float period = (uHidden > 0.5 ? 5.0 : 9.0) * uDpr;
    if (mod(s, period) > period * 0.55) discard;
  }
  outColor = vec4(vColor.rgb, vColor.a * cov);
}
"#;

const FACE_VS: &str = r#"
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec3 aMin;
layout(location=3) in vec3 aSize;
layout(location=4) in vec4 aColor;
layout(location=5) in vec3 aAnchor;
layout(location=6) in vec3 aParams; // near, far, flags
uniform float uDepthOnly;
out vec4 vColor;
out vec3 vWorld;
flat out float vHatch;
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  float flags = aParams.z;
  bool occ = flag(flags, 4.0);
  if (vis < 0.004 || (uDepthOnly > 0.5 && (!occ || vis < 0.5))) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec3 p = aMin + aPos * aSize;
  vWorld = p;
  gl_Position = uVP * vec4(p, 1.0);
  float shade = 0.7 + 0.3 * max(dot(aNormal, normalize(vec3(0.25, 1.0, 0.55))), 0.0);
  vColor = vec4(aColor.rgb * shade, aColor.a * vis * blinkMul(flags) * fogMul(p));
  vHatch = flag(flags, 8.0) ? 1.0 : 0.0;
}
"#;

const FACE_FS: &str = r#"
in vec4 vColor;
in vec3 vWorld;
flat in float vHatch;
out vec4 outColor;
void main() {
  if (vHatch > 0.5) {
    float v = (vWorld.x + vWorld.y) / 0.009;
    float f = abs(fract(v) - 0.5);
    float aa = fwidth(v) * 0.75;
    float stripe = 1.0 - smoothstep(0.12 - aa, 0.12 + aa, f);
    outColor = vec4(1.0, 0.54, 0.24, (0.04 + 0.28 * stripe) * vColor.a);
    return;
  }
  outColor = vColor;
}
"#;

const LED_VS: &str = r#"
layout(location=0) in vec2 aCorner;
layout(location=1) in vec4 aCenterSize;
layout(location=2) in vec4 aColor;
layout(location=3) in vec3 aAnchor;
layout(location=4) in vec4 aParams; // near, far, flags, min px
uniform float uProjY;
out vec4 vColor;
out vec2 vUv;
out float vPx;
flat out float vAlert;
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  // Lift the sprite towards the eye so its halo is not clipped by the panel it sits on.
  vec3 toEye = uEye - aCenterSize.xyz;
  vec3 lifted = aCenterSize.xyz + normalize(toEye) * min(aCenterSize.w * 3.0, length(toEye) * 0.5);
  vec4 c = uVP * vec4(lifted, 1.0);
  if (vis < 0.004 || c.w < 1e-3) { gl_Position = vec4(2.0, 2.0, 2.0, 1.0); return; }
  float pxPerWorld = uViewport.y * 0.5 * uProjY / c.w;
  float sizePx = max(aCenterSize.w * pxPerWorld, aParams.w * uDpr);
  bool alert = flag(aParams.z, 32.0);
  float r = sizePx * (alert ? 3.6 : 2.6) + 2.0;
  c.xy += aCorner * r / (0.5 * uViewport) * c.w;
  gl_Position = c;
  vUv = aCorner * r / sizePx;
  vPx = sizePx;
  vAlert = alert ? 1.0 : 0.0;
  float pulse = 1.0;
  if (alert && uMotion > 0.5) pulse = 0.8 + 0.2 * sin(uTime * 6.2831853 / 1800.0);
  vColor = vec4(aColor.rgb, vis * blinkMul(aParams.z) * fogMul(aCenterSize.xyz) * pulse);
}
"#;

const LED_FS: &str = r#"
in vec4 vColor;
in vec2 vUv;
in float vPx;
flat in float vAlert;
out vec4 outColor;
void main() {
  // Rounded-square core with a soft halo.
  vec2 q = abs(vUv) - vec2(0.32);
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - 0.18;
  float core = 1.0 - smoothstep(-1.0 / vPx, 1.0 / vPx, d);
  float r2 = dot(vUv, vUv);
  float halo = exp(-r2 * (vAlert > 0.5 ? 0.45 : 0.9)) * (vAlert > 0.5 ? 0.6 : 0.32);
  vec3 col = vColor.rgb * halo + mix(vColor.rgb, vec3(1.0), 0.18) * core * 0.9;
  outColor = vec4(col * vColor.a, 1.0);
}
"#;

const BG_VS: &str = r#"
out vec2 vNdc;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
  vNdc = p;
  gl_Position = vec4(p, 0.0, 1.0);
}
"#;

const BG_FS: &str = r#"
uniform mat4 uInvVP;
uniform vec4 uScene; // center xz, radius, unused
uniform float uWalk;
uniform vec3 uRoomMin;
uniform vec3 uRoomMax;
in vec2 vNdc;
out vec4 outColor;
float grid(vec2 p, float s, float w) {
  vec2 q = p / s;
  vec2 fw = fwidth(q);
  vec2 g = abs(fract(q - 0.5) - 0.5) / max(fw, vec2(1e-5));
  float l = min(g.x, g.y);
  float fade = 1.0 - smoothstep(0.12, 0.45, max(fw.x, fw.y));
  return (1.0 - min(l / w, 1.0)) * fade;
}
// Light pools under the ceiling fixtures (same 2.4 m grid as room.rs).
float pool(vec2 p, float r) {
  vec2 d = (fract((p - uRoomMin.xz) / 2.4) - 0.5) * 2.4 * vec2(0.55, 1.0);
  return exp(-dot(d, d) / r);
}
vec3 room(vec3 dir) {
  vec3 tA = (uRoomMin - uEye) / dir;
  vec3 tB = (uRoomMax - uEye) / dir;
  vec3 tf = max(tA, tB);
  float t = min(min(tf.x, tf.y), tf.z);
  vec3 hit = uEye + dir * t;
  vec3 col;
  if (t == tf.y && dir.y < 0.0) {
    // Raised floor: 600 mm tiles, light pools, a glow band under the rack fronts.
    col = vec3(0.035, 0.075, 0.130);
    col += vec3(0.07, 0.15, 0.24) * pool(hit.xz, 1.6);
    float tile = grid(hit.xz - uRoomMin.xz, 0.6, 1.3);
    col += vec3(0.25, 0.55, 0.85) * tile * 0.28;
    float fine = grid(hit.xz - uRoomMin.xz + 0.3, 0.6, 0.6);
    col += vec3(0.20, 0.45, 0.70) * fine * 0.04;
  } else if (t == tf.y) {
    // Ceiling: dim tiles, glow around fixtures.
    col = vec3(0.022, 0.045, 0.085);
    col += vec3(0.12, 0.22, 0.32) * pool(hit.xz, 0.5);
    col += vec3(0.20, 0.40, 0.65) * grid(hit.xz - uRoomMin.xz, 0.6, 1.0) * 0.10;
  } else {
    // Walls: panel joints every 1.2 m, brighter towards the light.
    vec2 w = t == tf.x ? hit.zy : hit.xy;
    col = mix(vec3(0.030, 0.062, 0.110), vec3(0.042, 0.085, 0.145), smoothstep(0.0, 3.0, hit.y));
    vec2 q = w / vec2(1.2, 1.2);
    vec2 fw = fwidth(q);
    vec2 g = abs(fract(q - 0.5) - 0.5) / max(fw, vec2(1e-5));
    col += vec3(0.20, 0.42, 0.65) * (1.0 - min(g.x / 1.0, 1.0)) * 0.12;
    col += vec3(0.25, 0.55, 0.85) * (1.0 - min(abs(hit.y - 0.12) / max(fwidth(hit.y), 1e-5), 1.0)) * 0.18;
  }
  float fogK = clamp((t - uFog.x) / max(uFog.y - uFog.x, 0.001), 0.0, 1.0);
  return mix(col, vec3(0.018, 0.038, 0.070), fogK * 0.8);
}
void main() {
  if (uWalk > 0.5) {
    vec4 a = uInvVP * vec4(vNdc, -1.0, 1.0);
    vec4 b = uInvVP * vec4(vNdc, 1.0, 1.0);
    vec3 dir = normalize(b.xyz / b.w - a.xyz / a.w);
    if (abs(dir.x) < 1e-5) dir.x = 1e-5;
    if (abs(dir.y) < 1e-5) dir.y = -1e-5;
    if (abs(dir.z) < 1e-5) dir.z = 1e-5;
    vec3 col = room(dir);
    vec2 v = vNdc * vec2(0.9, 1.0);
    col *= 1.0 - 0.4 * smoothstep(0.5, 1.6, dot(v, v));
    outColor = vec4(col, 1.0);
    return;
  }
  vec3 top = vec3(0.020, 0.043, 0.082);
  vec3 bot = vec3(0.035, 0.078, 0.140);
  vec3 col = mix(bot, top, smoothstep(-1.0, 1.0, vNdc.y));
  vec4 a = uInvVP * vec4(vNdc, -1.0, 1.0);
  vec4 b = uInvVP * vec4(vNdc, 1.0, 1.0);
  vec3 p0 = a.xyz / a.w;
  vec3 dir = normalize(b.xyz / b.w - p0);
  if (dir.y < -1e-4) {
    float t = -uEye.y / dir.y;
    vec3 hit = uEye + dir * t;
    float camD = distance(uEye, hit);
    float far = 1.0 - smoothstep(uFog.y * 0.6, uFog.y * 2.2, camD);
    float horizon = smoothstep(0.0, 0.12, -dir.y);
    float k = far * horizon;
    // Lighter blueprint paper under the installation.
    float r = distance(hit.xz, uScene.xy);
    col += vec3(0.020, 0.050, 0.090) * exp(-r / max(uScene.z, 1.0)) * k;
    float minor = grid(hit.xz, 0.1, 1.0);
    float major = grid(hit.xz, 1.0, 1.25);
    float sect = grid(hit.xz, 5.0, 1.6);
    col += vec3(0.20, 0.45, 0.70) * minor * 0.10 * k;
    col += vec3(0.30, 0.62, 0.90) * major * 0.22 * k;
    col += vec3(0.40, 0.75, 1.00) * sect * 0.18 * k;
    // Axes through the origin.
    vec2 ax = abs(hit.xz) / max(fwidth(hit.xz), vec2(1e-5));
    col += vec3(0.35, 0.7, 1.0) * (1.0 - min(min(ax.x, ax.y) / 1.2, 1.0)) * 0.12 * k;
  }
  // Vignette.
  vec2 v = vNdc * vec2(0.9, 1.0);
  col *= 1.0 - 0.35 * smoothstep(0.6, 1.6, dot(v, v));
  outColor = vec4(col, 1.0);
}
"#;

struct Prog {
    p: WebGlProgram,
    u: Vec<(&'static str, Option<WebGlUniformLocation>)>,
}

impl Prog {
    fn loc(&self, name: &str) -> Option<&WebGlUniformLocation> {
        self.u.iter().find(|(n, _)| *n == name).and_then(|(_, l)| l.as_ref())
    }
}

const UNIFORMS: [&str; 16] = [
    "uVP", "uEye", "uViewport", "uDpr", "uTime", "uMotion", "uFog", "uHidden", "uAlpha", "uDepthOnly", "uProjY", "uInvVP", "uScene", "uWalk", "uRoomMin",
    "uRoomMax",
];

struct Batch {
    vao: WebGlVertexArrayObject,
    buf: WebGlBuffer,
    count: i32,
}

pub struct Renderer {
    gl: GL,
    canvas: HtmlCanvasElement,
    bg: Prog,
    line: Prog,
    face: Prog,
    led: Prog,
    bg_vao: WebGlVertexArrayObject,
    lines: Batch,
    faces: Batch,
    leds: Batch,
    highlight: Batch,
    geo_version: u64,
    hl_version: u64,
    vp: [f32; 16],
}

fn compile(gl: &GL, kind: u32, src: &str) -> Result<WebGlShader, String> {
    let s = gl.create_shader(kind).ok_or("cannot create shader")?;
    gl.shader_source(&s, src);
    gl.compile_shader(&s);
    if gl.get_shader_parameter(&s, GL::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(s)
    } else {
        Err(gl.get_shader_info_log(&s).unwrap_or_default())
    }
}

fn program(gl: &GL, vs: &str, fs: &str) -> Result<Prog, String> {
    let head = "#version 300 es\nprecision highp float;\n";
    let v = compile(gl, GL::VERTEX_SHADER, &format!("{head}{COMMON}{vs}"))?;
    let f = compile(gl, GL::FRAGMENT_SHADER, &format!("{head}{COMMON}{fs}"))?;
    let p = gl.create_program().ok_or("cannot create program")?;
    gl.attach_shader(&p, &v);
    gl.attach_shader(&p, &f);
    gl.link_program(&p);
    if !gl.get_program_parameter(&p, GL::LINK_STATUS).as_bool().unwrap_or(false) {
        return Err(gl.get_program_info_log(&p).unwrap_or_default());
    }
    let u = UNIFORMS.iter().map(|n| (*n, gl.get_uniform_location(&p, n))).collect();
    Ok(Prog { p, u })
}

fn upload(gl: &GL, buf: &WebGlBuffer, data: &[f32], usage: u32) {
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(buf));
    // SAFETY: the view is consumed immediately, before any Rust allocation can move memory.
    unsafe {
        let view = js_sys::Float32Array::view(data);
        gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, usage);
    }
}

/// Static per-vertex buffer + instance buffer with the given attribute layout (loc, size, offset in floats).
fn batch(gl: &GL, mesh: &[f32], mesh_attrs: &[(u32, i32, i32)], mesh_stride: i32, inst_attrs: &[(u32, i32, i32)], inst_stride: usize) -> Result<Batch, String> {
    let vao = gl.create_vertex_array().ok_or("cannot create VAO")?;
    gl.bind_vertex_array(Some(&vao));
    let mbuf = gl.create_buffer().ok_or("cannot create buffer")?;
    upload(gl, &mbuf, mesh, GL::STATIC_DRAW);
    for &(loc, size, off) in mesh_attrs {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, size, GL::FLOAT, false, mesh_stride * 4, off * 4);
    }
    let buf = gl.create_buffer().ok_or("cannot create buffer")?;
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(&buf));
    for &(loc, size, off) in inst_attrs {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, size, GL::FLOAT, false, (inst_stride * 4) as i32, off * 4);
        gl.vertex_attrib_divisor(loc, 1);
    }
    gl.bind_vertex_array(None);
    Ok(Batch { vao, buf, count: 0 })
}

fn cube() -> Vec<f32> {
    // 6 faces × 2 triangles, position (0..1) + normal.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1., 0., 0.], [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]]),
        ([-1., 0., 0.], [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]]),
        ([0., 1., 0.], [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]]),
        ([0., -1., 0.], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]]),
        ([0., 0., 1.], [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]]),
        ([0., 0., -1.], [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]]),
    ];
    let mut v = Vec::with_capacity(36 * 6);
    for (n, q) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            v.extend_from_slice(&q[i]);
            v.extend_from_slice(&n);
        }
    }
    v
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement) -> Result<Renderer, String> {
        let opts = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&opts, &k.into(), &v);
        };
        set("antialias", true.into());
        set("alpha", false.into());
        set("depth", true.into());
        set("premultipliedAlpha", false.into());
        let gl: GL = canvas
            .get_context_with_context_options("webgl2", &opts)
            .map_err(|_| "WebGL2 is not available".to_string())?
            .ok_or("WebGL2 is not available in this browser")?
            .dyn_into()
            .map_err(|_| "WebGL2 is not available".to_string())?;

        let bg = program(&gl, BG_VS, BG_FS)?;
        let line = program(&gl, LINE_VS, LINE_FS)?;
        let face = program(&gl, FACE_VS, FACE_FS)?;
        let led = program(&gl, LED_VS, LED_FS)?;
        let bg_vao = gl.create_vertex_array().ok_or("cannot create VAO")?;

        let quad: [f32; 12] = [0., -1., 1., -1., 1., 1., 0., -1., 1., 1., 0., 1.];
        let line_attrs = [(1, 3, 0), (2, 3, 3), (3, 4, 6), (4, 3, 10), (5, 4, 13)];
        let lines = batch(&gl, &quad, &[(0, 2, 0)], 2, &line_attrs, LINE_STRIDE)?;
        let highlight = batch(&gl, &quad, &[(0, 2, 0)], 2, &line_attrs, LINE_STRIDE)?;
        let faces = batch(&gl, &cube(), &[(0, 3, 0), (1, 3, 3)], 6, &[(2, 3, 0), (3, 3, 3), (4, 4, 6), (5, 3, 10), (6, 3, 13)], FACE_STRIDE)?;
        let sq: [f32; 12] = [-1., -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.];
        let leds = batch(&gl, &sq, &[(0, 2, 0)], 2, &[(1, 4, 0), (2, 4, 4), (3, 3, 8), (4, 4, 11)], LED_STRIDE)?;

        Ok(Renderer {
            gl,
            canvas: canvas.clone(),
            bg,
            line,
            face,
            led,
            bg_vao,
            lines,
            faces,
            leds,
            highlight,
            geo_version: u64::MAX,
            hl_version: u64::MAX,
            vp: [0.0; 16],
        })
    }

    pub fn lost(&self) -> bool {
        self.gl.is_context_lost()
    }

    pub fn resize(&mut self, css_w: f32, css_h: f32, dpr: f32) {
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        if self.canvas.width() != w {
            self.canvas.set_width(w);
        }
        if self.canvas.height() != h {
            self.canvas.set_height(h);
        }
    }

    fn common(&self, p: &Prog, e: &Engine, vp: &[f32; 16]) {
        let gl = &self.gl;
        gl.use_program(Some(&p.p));
        gl.uniform_matrix4fv_with_f32_array(p.loc("uVP"), false, vp);
        let eye = e.cam.eye();
        gl.uniform3f(p.loc("uEye"), eye.x, eye.y, eye.z);
        gl.uniform2f(p.loc("uViewport"), self.canvas.width() as f32, self.canvas.height() as f32);
        gl.uniform1f(p.loc("uDpr"), e.dpr);
        gl.uniform1f(p.loc("uTime"), (e.now % 1.0e7) as f32);
        gl.uniform1f(p.loc("uMotion"), if e.reduced_motion { 0.0 } else { 1.0 });
        let r = e.layout.bounds.map(|b| b.radius()).unwrap_or(2.0);
        let (f0, f1) = e.cam.fog(r);
        gl.uniform2f(p.loc("uFog"), f0, f1);
    }

    pub fn draw(&mut self, e: &Engine) {
        let gl = &self.gl;
        if self.geo_version != e.geo_version {
            self.geo_version = e.geo_version;
            let g = &e.layout.geo;
            upload(gl, &self.lines.buf, &g.lines, GL::STATIC_DRAW);
            upload(gl, &self.faces.buf, &g.faces, GL::STATIC_DRAW);
            upload(gl, &self.leds.buf, &g.leds, GL::STATIC_DRAW);
            self.lines.count = g.line_count() as i32;
            self.faces.count = g.face_count() as i32;
            self.leds.count = g.led_count() as i32;
        }
        if self.hl_version != e.highlight_version {
            self.hl_version = e.highlight_version;
            upload(gl, &self.highlight.buf, &e.highlight.lines, GL::DYNAMIC_DRAW);
            self.highlight.count = e.highlight.line_count() as i32;
        }

        let (w, h) = (self.canvas.width() as i32, self.canvas.height() as i32);
        gl.viewport(0, 0, w, h);
        let vpm: Mat4 = e.cam.view_proj();
        self.vp = vpm.to_cols_array();
        let vp = self.vp;
        gl.depth_mask(true);
        gl.color_mask(true, true, true, true);
        gl.clear_color(0.03, 0.06, 0.11, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        // 1. Background + floor grid.
        gl.disable(GL::DEPTH_TEST);
        gl.disable(GL::BLEND);
        self.common(&self.bg, e, &vp);
        let inv = vpm.inverse().to_cols_array();
        gl.uniform_matrix4fv_with_f32_array(self.bg.loc("uInvVP"), false, &inv);
        let (c, r) = e.layout.bounds.map(|b| (b.center(), b.radius())).unwrap_or((glam::Vec3::ZERO, 3.0));
        gl.uniform4f(self.bg.loc("uScene"), c.x, c.z, r * 1.2, 0.0);
        match e.layout.room.as_ref() {
            Some(room) => {
                let (a, b) = (room.bounds.min, room.bounds.max);
                gl.uniform1f(self.bg.loc("uWalk"), 1.0);
                gl.uniform3f(self.bg.loc("uRoomMin"), a.x, a.y, a.z);
                gl.uniform3f(self.bg.loc("uRoomMax"), b.x, b.y, b.z);
            }
            None => gl.uniform1f(self.bg.loc("uWalk"), 0.0),
        }
        gl.bind_vertex_array(Some(&self.bg_vao));
        gl.draw_arrays(GL::TRIANGLES, 0, 3);

        // 2. Depth pre-pass from occluding faces (pushed back so their own edges stay visible).
        gl.enable(GL::DEPTH_TEST);
        gl.depth_func(GL::LEQUAL);
        gl.enable(GL::POLYGON_OFFSET_FILL);
        gl.polygon_offset(1.5, 4.0);
        if self.faces.count > 0 {
            gl.color_mask(false, false, false, false);
            self.common(&self.face, e, &vp);
            gl.uniform1f(self.face.loc("uDepthOnly"), 1.0);
            gl.bind_vertex_array(Some(&self.faces.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 36, self.faces.count);
            gl.color_mask(true, true, true, true);

            // 3. Translucent faces.
            gl.depth_mask(false);
            gl.enable(GL::BLEND);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            gl.uniform1f(self.face.loc("uDepthOnly"), 0.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 36, self.faces.count);
        }
        gl.disable(GL::POLYGON_OFFSET_FILL);
        gl.depth_mask(false);
        gl.enable(GL::BLEND);
        gl.blend_func(GL::SRC_ALPHA, GL::ONE);

        // 4. Lines: hidden (dashed, faint) then visible.
        if self.lines.count > 0 {
            self.common(&self.line, e, &vp);
            gl.bind_vertex_array(Some(&self.lines.vao));
            gl.depth_func(GL::GREATER);
            gl.uniform1f(self.line.loc("uHidden"), 1.0);
            gl.uniform1f(self.line.loc("uAlpha"), 0.16);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
            gl.depth_func(GL::LEQUAL);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
        }

        // 5. LEDs (additive glow, occluded by solid parts).
        if self.leds.count > 0 {
            self.common(&self.led, e, &vp);
            let proj = e.cam.proj();
            gl.uniform1f(self.led.loc("uProjY"), proj.y_axis.y);
            gl.blend_func(GL::ONE, GL::ONE);
            gl.bind_vertex_array(Some(&self.leds.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.leds.count);
        }

        // 6. Hover / selection outlines on top of everything.
        if self.highlight.count > 0 {
            gl.disable(GL::DEPTH_TEST);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            self.common(&self.line, e, &vp);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.bind_vertex_array(Some(&self.highlight.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.highlight.count);
        }
        gl.bind_vertex_array(None);
    }
}
